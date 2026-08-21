use std::collections::HashMap;

use crate::incremental::{json_array_text, quote_identifier};
use crate::program::GenProgram;
use crate::sql::{column_index, result_rows, SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, BoundaryResult, OrderedEdgeArm, OrderedTriggerKind, RelDelta,
    RelationKind, Row, ScalarSeam, ScalarValue, SqlStatement, TickDeltas,
};

type Snapshot = HashMap<String, Vec<Row>>;

#[derive(Clone)]
struct Occurrence {
    rel: String,
    kind: OrderedTriggerKind,
    row: Row,
    sequence: Option<i64>,
}

#[derive(Clone)]
struct OrderedWrite {
    arm_index: usize,
    row: Row,
}

fn read_snapshot(
    program: &GenProgram,
    seam: &SqliteSeam,
    decoded: bool,
) -> BoundaryResult<Snapshot> {
    let mut snapshot = HashMap::new();
    for relation in &program.relations {
        let sql = if decoded {
            program
                .final_select
                .get(&relation.rel)
                .cloned()
                .expect("decoded snapshot SQL missing")
        } else {
            let columns = relation
                .columns
                .iter()
                .map(|column| quote_identifier(column))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "SELECT {} FROM {}",
                columns,
                quote_identifier(&relation.table_name)
            )
        };
        let result = seam
            .execute(&SqlStatement { sql, args: vec![] })
            .expect("ordered snapshot read failed");
        let rows = result_rows(&result, &relation.columns, &relation.column_types)?;
        let rows = if decoded {
            rows.iter()
                .map(|row| {
                    crate::enum_plane::decode_row(
                        seam,
                        &program.enum_types,
                        &program.enum_ref_columns,
                        &program.relations,
                        &relation.rel,
                        row,
                    )
                })
                .collect::<BoundaryResult<Vec<_>>>()?
        } else {
            rows
        };
        snapshot.insert(relation.rel.clone(), rows);
    }
    Ok(snapshot)
}

/// One comparison per row under an index, one per pair under a scan.
static DIFF_PROBES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn diff_probes() -> u64 {
    DIFF_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Insertion order is kept because the tick log is graded byte-for-byte and a
/// hash order would move the rows a diff emits.
fn row_counts(rows: &[Row]) -> Vec<(Row, usize)> {
    let mut counts: Vec<(Row, usize)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for row in rows {
        DIFF_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match index.entry(crate::incremental::dedup_key(row)) {
            std::collections::hash_map::Entry::Occupied(seen) => counts[*seen.get()].1 += 1,
            std::collections::hash_map::Entry::Vacant(fresh) => {
                fresh.insert(counts.len());
                counts.push((row.clone(), 1));
            }
        }
    }
    counts
}

pub fn multiset_diff(before: &[Row], after: &[Row]) -> (Vec<Row>, Vec<Row>) {
    let before_counts = row_counts(before);
    let after_counts = row_counts(after);
    let count_of = |counts: &[(Row, usize)]| -> HashMap<String, usize> {
        counts
            .iter()
            .map(|(row, count)| {
                DIFF_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                (crate::incremental::dedup_key(row), *count)
            })
            .collect()
    };
    let before_index = count_of(&before_counts);
    let after_index = count_of(&after_counts);
    let mut add = Vec::new();
    for (row, count) in &after_counts {
        DIFF_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let prior = before_index
            .get(&crate::incremental::dedup_key(row))
            .copied()
            .unwrap_or(0);
        for _ in prior..*count {
            add.push(row.clone());
        }
    }
    let mut del = Vec::new();
    for (row, count) in &before_counts {
        DIFF_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let next = after_index
            .get(&crate::incremental::dedup_key(row))
            .copied()
            .unwrap_or(0);
        for _ in next..*count {
            del.push(row.clone());
        }
    }
    (add, del)
}

fn build_deltas(program: &GenProgram, before: &Snapshot, after: &Snapshot) -> Vec<RelDelta> {
    program
        .relations
        .iter()
        .map(|relation| {
            let (add, del) = multiset_diff(
                before.get(&relation.rel).map(Vec::as_slice).unwrap_or(&[]),
                after.get(&relation.rel).map(Vec::as_slice).unwrap_or(&[]),
            );
            RelDelta {
                rel: relation.rel.clone(),
                add,
                del,
            }
        })
        .collect()
}

fn apply_arrivals(
    program: &GenProgram,
    seam: &SqliteSeam,
    arrivals: &[Arrival],
) -> BoundaryResult<()> {
    let statements = arrivals
        .iter()
        .map(|arrival| {
            let template = program
                .arrival_templates
                .get(&arrival.rel)
                .unwrap_or_else(|| panic!("arrival template missing for {}", arrival.rel));
            let sql = match arrival.sign {
                ArrivalSign::Add => template.add_sql.clone(),
                ArrivalSign::Del if template.kind == RelationKind::Log => {
                    panic!("retract from log rel {}", arrival.rel)
                }
                ArrivalSign::Del => template
                    .del_sql
                    .clone()
                    .unwrap_or_else(|| panic!("delete template missing for {}", arrival.rel)),
            };
            Ok(SqlStatement {
                sql,
                args: ScalarValue::row_at_seam(&arrival.row, ScalarSeam::SqlParameter)?,
            })
        })
        .collect::<BoundaryResult<Vec<_>>>()?;
    if !statements.is_empty() {
        seam.batch(&statements).expect("ordered arrivals failed");
    }
    Ok(())
}

fn snapshot_pre(program: &GenProgram, seam: &SqliteSeam) {
    let mut statements = Vec::new();
    for name in &program.ordered_pre_refs {
        let relation = program
            .relations
            .iter()
            .find(|relation| relation.rel == *name)
            .unwrap_or_else(|| panic!("ordered pre relation missing: {name}"));
        let columns = relation
            .columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        let pre_table = quote_identifier(&format!("__pre_{}", relation.table_name));
        statements.push(format!("DELETE FROM {pre_table}"));
        statements.push(format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            pre_table,
            columns,
            columns,
            quote_identifier(&relation.table_name)
        ));
    }
    if !statements.is_empty() {
        seam.execute_multiple(&statements.join(";\n"))
            .expect("ordered pre snapshot failed");
    }
}

fn recompute_levels(program: &GenProgram, seam: &SqliteSeam) {
    if program.ordered_recursive_levels {
        for statement in &program.levels {
            seam.execute_multiple(&statement.recompute_delete_sql)
                .expect("ordered level clear failed");
        }
        let count_sql = format!(
            "SELECT {}",
            program
                .levels
                .iter()
                .map(|statement| format!(
                    "(SELECT count(*) FROM {})",
                    quote_identifier(&statement.head_table_name)
                ))
                .collect::<Vec<_>>()
                .join(" + ")
        );
        let mut prior = -1;
        loop {
            for statement in &program.levels {
                for sql in &statement.recompute_insert_sqls {
                    seam.execute_multiple(sql)
                        .expect("ordered recursive level insert failed");
                }
            }
            let rows = seam
                .scalar(&count_sql)
                .expect("ordered recursive level count failed");
            if rows == prior {
                break;
            }
            prior = rows;
        }
        return;
    }
    for statement in &program.levels {
        seam.execute_multiple(&statement.recompute_sql)
            .expect("ordered level recompute failed");
    }
}

fn apply_retention(program: &GenProgram, seam: &SqliteSeam) {
    let statements = program
        .retentions
        .iter()
        .map(|retention| SqlStatement {
            sql: retention.delete_sql.clone(),
            args: vec![],
        })
        .collect::<Vec<_>>();
    if !statements.is_empty() {
        seam.batch(&statements).expect("ordered retention failed");
    }
}

fn trigger_relations(program: &GenProgram, kind: OrderedTriggerKind) -> Vec<String> {
    let mut names = program
        .ordered_arms
        .iter()
        .filter(|arm| arm.trigger_kind == kind)
        .map(|arm| arm.trigger_rel.clone())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn read_carry(program: &GenProgram, seam: &SqliteSeam) -> Vec<Occurrence> {
    let mut occurrences = Vec::new();
    for name in trigger_relations(program, OrderedTriggerKind::Arrival) {
        let relation = program
            .relations
            .iter()
            .find(|relation| relation.rel == name)
            .expect("ordered carry relation missing");
        let columns = relation
            .columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        let result = seam
            .execute(&SqlStatement {
                sql: format!(
                    "SELECT \"_sequence\" AS \"__sequence\", {} FROM {} ORDER BY \"_phase\", \"_sequence\"",
                    columns,
                    quote_identifier(&relation.frontier_table_name)
                ),
                args: vec![],
            })
            .expect("ordered carry read failed");
        let sequence_index = column_index(&result, "__sequence").expect("carry sequence missing");
        for row in &result.rows {
            let values = relation
                .columns
                .iter()
                .map(|column| {
                    let index = column_index(&result, column).expect("carry column missing");
                    row[index].clone()
                })
                .collect();
            occurrences.push(Occurrence {
                rel: name.clone(),
                kind: OrderedTriggerKind::Arrival,
                row: values,
                sequence: row[sequence_index].as_i64(),
            });
        }
    }
    occurrences.sort_by_key(|occurrence| occurrence.sequence.unwrap_or(0));
    occurrences
}

fn read_departures(program: &GenProgram, seam: &SqliteSeam) -> BoundaryResult<Vec<Occurrence>> {
    let mut occurrences = Vec::new();
    for name in trigger_relations(program, OrderedTriggerKind::Departure) {
        let relation = program
            .relations
            .iter()
            .find(|relation| relation.rel == name)
            .expect("ordered departure relation missing");
        let table = relation
            .departure_frontier_table_name
            .as_ref()
            .expect("ordered departure table missing");
        let columns = relation
            .columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        let result = seam
            .execute(&SqlStatement {
                sql: format!(
                    "SELECT {} FROM {} ORDER BY \"_phase\", \"_sequence\"",
                    columns,
                    quote_identifier(table)
                ),
                args: vec![],
            })
            .expect("ordered departure read failed");
        for row in result_rows(&result, &relation.columns, &relation.column_types)? {
            occurrences.push(Occurrence {
                rel: name.clone(),
                kind: OrderedTriggerKind::Departure,
                row,
                sequence: None,
            });
        }
    }
    Ok(occurrences)
}

fn outside_occurrences(
    program: &GenProgram,
    before: &Snapshot,
    arrivals: &[Arrival],
) -> Vec<Occurrence> {
    let trigger_names = trigger_relations(program, OrderedTriggerKind::Arrival);
    let mut seen_by_rel: HashMap<&str, Vec<Row>> = HashMap::new();
    for name in &trigger_names {
        let relation = program
            .relations
            .iter()
            .find(|relation| relation.rel == *name)
            .expect("ordered trigger relation missing");
        if relation.kind == RelationKind::Set {
            seen_by_rel.insert(name, before.get(name).cloned().unwrap_or_default());
        }
    }
    let mut occurrences = Vec::new();
    for arrival in arrivals {
        if arrival.sign != ArrivalSign::Add || !trigger_names.contains(&arrival.rel) {
            continue;
        }
        let relation = program
            .relations
            .iter()
            .find(|relation| relation.rel == arrival.rel)
            .expect("ordered trigger relation missing");
        if relation.kind == RelationKind::Set {
            let seen = seen_by_rel.entry(&arrival.rel).or_default();
            if seen.contains(&arrival.row) {
                continue;
            }
            seen.push(arrival.row.clone());
        }
        occurrences.push(Occurrence {
            rel: arrival.rel.clone(),
            kind: OrderedTriggerKind::Arrival,
            row: arrival.row.clone(),
            sequence: None,
        });
    }
    occurrences
}

fn level_occurrences(program: &GenProgram, before: &Snapshot, mid: &Snapshot) -> Vec<Occurrence> {
    let trigger_names = trigger_relations(program, OrderedTriggerKind::Arrival);
    let mut level_names = program
        .levels
        .iter()
        .map(|level| level.head_rel.clone())
        .collect::<Vec<_>>();
    level_names.sort();
    level_names.dedup();
    let mut occurrences = Vec::new();
    for name in level_names {
        if !trigger_names.contains(&name) {
            continue;
        }
        let (add, _) = multiset_diff(
            before.get(&name).map(Vec::as_slice).unwrap_or(&[]),
            mid.get(&name).map(Vec::as_slice).unwrap_or(&[]),
        );
        for row in add {
            occurrences.push(Occurrence {
                rel: name.clone(),
                kind: OrderedTriggerKind::Arrival,
                row,
                sequence: None,
            });
        }
    }
    occurrences
}

fn pre_write_statement(arm: &OrderedEdgeArm, row: &[ScalarValue]) -> Option<SqlStatement> {
    if !arm.evolves_pre {
        return None;
    }
    let table = quote_identifier(&format!("__pre_{}", arm.head_table_name));
    let columns = arm
        .head_columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>();
    let placeholders = vec!["?"; columns.len()].join(", ");
    if arm.head_kind == RelationKind::Log {
        return Some(SqlStatement {
            sql: format!(
                "INSERT INTO {} ({}) VALUES ({})",
                table,
                columns.join(", "),
                placeholders
            ),
            args: row.to_vec(),
        });
    }
    let key_columns = arm
        .key_indices
        .iter()
        .map(|index| columns[*index].clone())
        .collect::<Vec<_>>();
    let non_key_columns = columns
        .iter()
        .enumerate()
        .filter(|(index, _)| !arm.key_indices.contains(index))
        .map(|(_, column)| column.clone())
        .collect::<Vec<_>>();
    let conflict = if non_key_columns.is_empty() {
        format!("ON CONFLICT({}) DO NOTHING", key_columns.join(", "))
    } else {
        format!(
            "ON CONFLICT({}) DO UPDATE SET {}",
            key_columns.join(", "),
            non_key_columns
                .iter()
                .map(|column| format!("{} = excluded.{}", column, column))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Some(SqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES ({}) {}",
            table,
            columns.join(", "),
            placeholders,
            conflict
        ),
        args: row.to_vec(),
    })
}

fn apply_occurrence(
    program: &GenProgram,
    seam: &SqliteSeam,
    occurrence: &Occurrence,
    written: &mut Vec<OrderedWrite>,
) -> BoundaryResult<()> {
    let trigger_args = ScalarValue::row_at_seam(&occurrence.row, ScalarSeam::SqlParameter)?;
    let mut projected = Vec::new();
    for (arm_index, arm) in program.ordered_arms.iter().enumerate() {
        if arm.trigger_rel != occurrence.rel || arm.trigger_kind != occurrence.kind {
            continue;
        }
        for sql in arm.intern_sql.as_deref().unwrap_or(&[]) {
            seam.execute(&SqlStatement {
                sql: sql.clone(),
                args: trigger_args.clone(),
            })
            .expect("ordered intern failed");
        }
        let result = seam
            .execute(&SqlStatement {
                sql: arm.project_sql.clone(),
                args: trigger_args.clone(),
            })
            .expect("ordered projection failed");
        for result_row in &result.rows {
            let row = arm
                .head_columns
                .iter()
                .map(|column| {
                    let index = column_index(&result, column).expect("ordered head column missing");
                    result_row[index].clone()
                })
                .collect::<Row>();
            projected.push(OrderedWrite { arm_index, row });
        }
    }
    let mut writes: Vec<OrderedWrite> = Vec::new();
    let mut keyed: Vec<(String, Row)> = Vec::new();
    for write in projected {
        let arm = &program.ordered_arms[write.arm_index];
        if writes.iter().any(|prior| {
            program.ordered_arms[prior.arm_index].head_rel == arm.head_rel && prior.row == write.row
        }) {
            continue;
        }
        if arm.head_kind == RelationKind::Set {
            let key = format!(
                "{}:{}",
                arm.head_rel,
                json_array_text(
                    &arm.key_indices
                        .iter()
                        .map(|index| write.row[*index].clone())
                        .collect::<Vec<_>>()
                )?
            );
            if let Some((_, prior)) = keyed.iter().find(|(prior_key, _)| prior_key == &key) {
                if prior != &write.row {
                    panic!("keyed conflict in ordered occurrence for {}", arm.head_rel);
                }
            } else {
                keyed.push((key, write.row.clone()));
            }
        }
        writes.push(write);
    }
    let mut statements = Vec::new();
    for write in &writes {
        let arm = &program.ordered_arms[write.arm_index];
        let args = ScalarValue::row_at_seam(&write.row, ScalarSeam::SqlParameter)?;
        statements.push(SqlStatement {
            sql: arm.write_sql.clone(),
            args: args.clone(),
        });
        if let Some(pre) = pre_write_statement(arm, &args) {
            statements.push(pre);
        }
    }
    if !statements.is_empty() {
        seam.batch(&statements).expect("ordered writes failed");
        written.extend(writes);
    }
    Ok(())
}

fn carry_additions(
    program: &GenProgram,
    mid: &Snapshot,
    after: &Snapshot,
    boundary: &[RelDelta],
    written: &[OrderedWrite],
) -> Vec<RelDelta> {
    let mut additions = Vec::new();
    let mut seen: Vec<(String, Row)> = Vec::new();
    for write in written {
        let arm = &program.ordered_arms[write.arm_index];
        let visible = boundary
            .iter()
            .find(|delta| delta.rel == arm.head_rel)
            .map(|delta| delta.add.contains(&write.row))
            .unwrap_or(false);
        if !visible || seen.contains(&(arm.head_rel.clone(), write.row.clone())) {
            continue;
        }
        seen.push((arm.head_rel.clone(), write.row.clone()));
        additions.push(RelDelta {
            rel: arm.head_rel.clone(),
            add: vec![write.row.clone()],
            del: vec![],
        });
    }
    let mut level_names = program
        .levels
        .iter()
        .map(|level| level.head_rel.clone())
        .collect::<Vec<_>>();
    level_names.sort();
    level_names.dedup();
    for name in level_names {
        let (add, _) = multiset_diff(
            mid.get(&name).map(Vec::as_slice).unwrap_or(&[]),
            after.get(&name).map(Vec::as_slice).unwrap_or(&[]),
        );
        for row in add {
            let visible = boundary
                .iter()
                .find(|delta| delta.rel == name)
                .map(|delta| delta.add.contains(&row))
                .unwrap_or(false);
            if !visible || seen.contains(&(name.clone(), row.clone())) {
                continue;
            }
            seen.push((name.clone(), row.clone()));
            additions.push(RelDelta {
                rel: name.clone(),
                add: vec![row],
                del: vec![],
            });
        }
    }
    additions
}

fn base_carry_pending(program: &GenProgram, deltas: &[RelDelta]) -> bool {
    let mut head_names = program
        .ordered_arms
        .iter()
        .map(|arm| arm.head_rel.as_str())
        .collect::<Vec<_>>();
    head_names.sort();
    head_names.dedup();
    if deltas.iter().any(|delta| {
        head_names.contains(&delta.rel.as_str()) && (!delta.add.is_empty() || !delta.del.is_empty())
    }) {
        return true;
    }
    let departure_names = trigger_relations(program, OrderedTriggerKind::Departure);
    deltas
        .iter()
        .any(|delta| departure_names.contains(&delta.rel) && !delta.del.is_empty())
}

pub fn run_tick(
    program: &GenProgram,
    seam: &SqliteSeam,
    arrivals: &[Arrival],
) -> BoundaryResult<TickDeltas> {
    let before_decoded = read_snapshot(program, seam, true)?;
    let before_stored = read_snapshot(program, seam, false)?;
    if program.uses_tick {
        crate::incremental::advance_tick(seam);
    }
    let enumed = crate::enum_plane::intern(
        seam,
        &program.enum_types,
        &program.enum_ref_columns,
        arrivals,
    )?;
    let interned = match &program.text_intern_plan {
        Some(plan) => crate::text_plane::intern(seam, plan, &enumed)?,
        None => enumed,
    };
    let normalized = crate::struct_plane::intern(
        seam,
        &program.struct_types,
        &program.struct_ref_columns,
        &interned,
        &program.relations,
        program.text_intern_plan.as_ref(),
    )?;
    apply_arrivals(program, seam, &normalized)?;
    snapshot_pre(program, seam);
    recompute_levels(program, seam);
    let mid = read_snapshot(program, seam, false)?;
    let mut occurrences = read_carry(program, seam);
    occurrences.extend(read_departures(program, seam)?);
    occurrences.extend(outside_occurrences(program, &before_stored, &normalized));
    occurrences.extend(level_occurrences(program, &before_stored, &mid));
    let mut written = Vec::new();
    for occurrence in &occurrences {
        apply_occurrence(program, seam, occurrence, &mut written)?;
    }
    recompute_levels(program, seam);
    apply_retention(program, seam);
    let after_decoded = read_snapshot(program, seam, true)?;
    let after_stored = read_snapshot(program, seam, false)?;
    let deltas = build_deltas(program, &before_decoded, &after_decoded);
    let stored_deltas = build_deltas(program, &before_stored, &after_stored);
    let additions = carry_additions(program, &mid, &after_stored, &stored_deltas, &written);
    let ordered_carry =
        crate::incremental::stage_ordered_frontiers(seam, &program.relations, &additions)?;
    // Decoded rows, not stored ids: read_departures types the frontier table
    // back through the rel's declared column types, and the TypeScript door
    // stages the same decoded delta (emit_ts.pl snapshot_departure_stage_lines).
    crate::incremental::stage_departures(seam, &program.relations, &deltas)?;
    Ok(TickDeltas {
        carry_pending: base_carry_pending(program, &deltas) || ordered_carry,
        rels: deltas,
    })
}
