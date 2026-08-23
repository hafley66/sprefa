use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use crate::incremental::{json_array_text, quote_identifier};
use crate::program::GenProgram;
use crate::sql::{column_index, result_rows, SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalSign, BoundaryResult, IncrementalRelationPlan, OrderedEdgeArm,
    OrderedTriggerKind, RelDelta, RelationKind, Row, ScalarSeam, ScalarValue, SqlStatement,
    TickDeltas,
};

type Snapshot = HashMap<String, Vec<Row>>;

/// Columns in one probe statement, under SQLite's expression depth and column
/// count limits.
const PROBE_WIDTH: usize = 100;

/// Which rels each level's recompute reads, and the probe texts a tick opens
/// with. Derived once per program: the scan must never run per tick.
struct OrderedPlan {
    level_reads: Vec<Vec<usize>>,
    level_always: Vec<bool>,
    /// Level index -> which of its insert statements write the head table.
    level_head_inserts: Vec<Vec<usize>>,
    /// The tick's opening probe. Columns: one per rel for a live frontier, one
    /// per rel for a non-empty base table, then one for any level head at all.
    probe: Vec<String>,
    /// The rels the struct plane interns into, ahead of the ordered arrivals.
    struct_rels: Vec<String>,
    level_heads: HashSet<String>,
    /// The rels `carry_additions` reads the stored before and after of.
    carry_rels: HashSet<String>,
}

/// What the opening probe answers, all of it read in one chunked statement set.
struct TickProbe {
    live: HashSet<String>,
    empty: HashSet<String>,
}

/// A read whose table is no rel's base table (a frontier, a `__pre_` snapshot, a
/// plane table, a CTE) leaves the read set unknown, and unknown never skips.
fn reads_unknown_table(sql: &str, known: &HashSet<&str>) -> bool {
    for keyword in ["FROM ", "JOIN "] {
        for (at, _) in sql.match_indices(keyword) {
            let rest = sql[at + keyword.len()..].trim_start();
            if rest.starts_with('(') {
                continue;
            }
            let Some(rest) = rest.strip_prefix('"') else {
                return true;
            };
            let Some(end) = rest.find('"') else {
                return true;
            };
            if &rest[..end] != "__str" && !known.contains(&rest[..end]) {
                return true;
            }
        }
    }
    false
}

impl OrderedPlan {
    fn of(program: &GenProgram) -> OrderedPlan {
        let bases: Vec<(String, &str)> = program
            .relations
            .iter()
            .map(|relation| {
                (
                    quote_identifier(&relation.table_name),
                    relation.rel.as_str(),
                )
            })
            .collect();
        let known: HashSet<&str> = program
            .relations
            .iter()
            .map(|relation| relation.table_name.as_str())
            .collect();
        let reads = crate::incremental::reads_by_head(
            program.levels.iter().flat_map(|level| {
                level
                    .recompute_insert_sqls
                    .iter()
                    .map(|sql| (level.head_rel.as_str(), Some(sql.as_str())))
            }),
            &bases,
        );
        let rel_index: HashMap<&str, usize> = program
            .relations
            .iter()
            .enumerate()
            .map(|(index, relation)| (relation.rel.as_str(), index))
            .collect();
        // Retention runs after the last recompute of its tick, so a level over a
        // retained rel is stale at the next tick unless it always recomputes.
        let retained: HashSet<&str> = program
            .retentions
            .iter()
            .map(|retention| retention.rel.as_str())
            .collect();
        let mut level_reads = Vec::with_capacity(program.levels.len());
        let mut level_always = Vec::with_capacity(program.levels.len());
        let mut level_head_inserts = Vec::with_capacity(program.levels.len());
        for level in &program.levels {
            let sources = reads
                .iter()
                .find(|(head, _)| *head == level.head_rel)
                .map(|(_, sources)| sources.as_slice())
                .unwrap_or(&[]);
            let mut indices: Vec<usize> = sources
                .iter()
                .filter(|rel| **rel != level.head_rel)
                .filter_map(|rel| rel_index.get(rel.as_str()).copied())
                .collect();
            indices.sort_unstable();
            indices.dedup();
            level_always.push(
                sources.iter().any(|rel| retained.contains(rel.as_str()))
                    || level
                        .recompute_insert_sqls
                        .iter()
                        .any(|sql| reads_unknown_table(sql, &known)),
            );
            level_reads.push(indices);
            let head_table = quote_identifier(&level.head_table_name);
            level_head_inserts.push(
                level
                    .recompute_insert_sqls
                    .iter()
                    .enumerate()
                    .filter(|(_, sql)| sql.contains(&format!("INTO {head_table}")))
                    .map(|(index, _)| index)
                    .collect(),
            );
        }
        let mut columns: Vec<String> = program.relations.iter().map(frontier_exists).collect();
        columns.extend(program.relations.iter().map(|relation| {
            format!(
                "EXISTS(SELECT 1 FROM {})",
                quote_identifier(&relation.table_name)
            )
        }));
        // An empty column list is not a SELECT, and a program with no rels has
        // nothing to ask about.
        columns.push("1".to_string());
        let probe = columns
            .chunks(PROBE_WIDTH)
            .map(|chunk| format!("SELECT {}", chunk.join(", ")))
            .collect();
        let struct_rels = program
            .struct_types
            .iter()
            .filter_map(|structure| {
                program
                    .relations
                    .iter()
                    .find(|relation| {
                        structure
                            .intern_sql
                            .contains(&quote_identifier(&relation.table_name))
                    })
                    .map(|relation| relation.rel.clone())
            })
            .collect();
        let level_heads: HashSet<String> = program
            .levels
            .iter()
            .map(|level| level.head_rel.clone())
            .collect();
        let mut carry_rels = level_heads.clone();
        carry_rels.extend(program.ordered_arms.iter().map(|arm| arm.head_rel.clone()));
        OrderedPlan {
            level_reads,
            level_always,
            level_head_inserts,
            probe,
            struct_rels,
            level_heads,
            carry_rels,
        }
    }
}

fn frontier_exists(relation: &IncrementalRelationPlan) -> String {
    let mut tables = vec![
        relation.frontier_table_name.clone(),
        relation.next_frontier_table_name.clone(),
    ];
    tables.extend(relation.departure_frontier_table_name.clone());
    tables
        .iter()
        .map(|table| format!("EXISTS(SELECT 1 FROM {})", quote_identifier(table)))
        .collect::<Vec<_>>()
        .join(" OR ")
}

static ORDERED_PLANS: OnceLock<Mutex<HashMap<u64, Arc<OrderedPlan>>>> = OnceLock::new();

fn ordered_plan(program: &GenProgram) -> Arc<OrderedPlan> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    program.name.hash(&mut hasher);
    program.relations.len().hash(&mut hasher);
    for relation in &program.relations {
        relation.rel.hash(&mut hasher);
    }
    for level in &program.levels {
        level.head_rel.hash(&mut hasher);
    }
    let plans = ORDERED_PLANS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut plans = plans.lock().expect("ordered plan cache");
    Arc::clone(
        plans
            .entry(hasher.finish())
            .or_insert_with(|| Arc::new(OrderedPlan::of(program))),
    )
}

/// The rels that moved this tick and the rows each held before its first write.
/// The generation orders the marks, so a level runs when a rel it reads moved.
struct TickDirty {
    generation: u64,
    marked: HashMap<String, u64>,
    decoded: Snapshot,
    stored: Snapshot,
    /// Rels whose table held no row when the tick opened.
    empty: HashSet<String>,
}

impl TickDirty {
    fn new(empty: HashSet<String>) -> TickDirty {
        TickDirty {
            generation: 0,
            marked: HashMap::new(),
            decoded: HashMap::new(),
            stored: HashMap::new(),
            empty,
        }
    }

    /// Read once, before the rel's first write of the tick: after the write the
    /// rows it held are gone.
    fn arm(&mut self, program: &GenProgram, seam: &SqliteSeam, rel: &str) -> BoundaryResult<()> {
        if self.stored.contains_key(rel) {
            return Ok(());
        }
        // Nothing writes a rel before its own arm, so a table empty when the
        // tick opened is still empty here and needs no read.
        if self.empty.contains(rel) {
            self.decoded.insert(rel.to_string(), Vec::new());
            self.stored.insert(rel.to_string(), Vec::new());
            return Ok(());
        }
        let relation = relation_of(program, rel);
        let decoded = read_relation(program, seam, relation, true)?;
        let stored = read_relation(program, seam, relation, false)?;
        self.decoded.insert(rel.to_string(), decoded);
        self.stored.insert(rel.to_string(), stored);
        Ok(())
    }

    fn mark(&mut self, rel: &str, rows_changed: i64) {
        if rows_changed <= 0 {
            return;
        }
        debug_assert!(
            self.stored.contains_key(rel),
            "ordered tick marked {rel} without arming it"
        );
        self.generation += 1;
        self.marked.insert(rel.to_string(), self.generation);
    }

    fn newest(&self, program: &GenProgram, rels: &[usize]) -> u64 {
        rels.iter()
            .filter_map(|index| self.marked.get(&program.relations[*index].rel))
            .copied()
            .max()
            .unwrap_or(0)
    }

    /// The rels whose rows moved, sorted: a hash order would move the reads.
    fn moved(&self) -> Vec<String> {
        let mut rels: Vec<String> = self.marked.keys().cloned().collect();
        rels.sort();
        rels
    }

    fn moved_within(&self, wanted: &HashSet<String>) -> Vec<String> {
        let mut rels: Vec<String> = self
            .marked
            .keys()
            .filter(|rel| wanted.contains(*rel))
            .cloned()
            .collect();
        rels.sort();
        rels
    }
}

fn relation_of<'a>(program: &'a GenProgram, rel: &str) -> &'a IncrementalRelationPlan {
    program
        .relations
        .iter()
        .find(|relation| relation.rel == rel)
        .unwrap_or_else(|| panic!("ordered relation missing: {rel}"))
}

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

fn read_relation(
    program: &GenProgram,
    seam: &SqliteSeam,
    relation: &IncrementalRelationPlan,
    decoded: bool,
) -> BoundaryResult<Vec<Row>> {
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
    let _scope = crate::trace::Scope::verb("snapshot", &relation.rel, "ordered");
    let result = seam
        .execute(&SqlStatement { sql, args: vec![] })
        .expect("ordered snapshot read failed");
    let rows = result_rows(&result, &relation.columns, &relation.column_types)?;
    if !decoded {
        return Ok(rows);
    }
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
        .collect()
}

/// The named rels only: a rel that no writer touched this tick has the rows it
/// had before, and its delta is empty without a read.
fn read_snapshot_of(
    program: &GenProgram,
    seam: &SqliteSeam,
    decoded: bool,
    rels: &[String],
) -> BoundaryResult<Snapshot> {
    let mut snapshot = HashMap::new();
    for rel in rels {
        let rows = read_relation(program, seam, relation_of(program, rel), decoded)?;
        snapshot.insert(rel.clone(), rows);
    }
    Ok(snapshot)
}

/// Levels whose recompute ran, so a test can read that the first tick against a
/// db rebuilt every one of them.
static LEVEL_RECOMPUTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn level_recomputes() -> u64 {
    LEVEL_RECOMPUTES.load(std::sync::atomic::Ordering::Relaxed)
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

/// A rel absent from `after` was never written this tick, so its delta is empty
/// without a diff.
fn build_deltas(program: &GenProgram, before: &Snapshot, after: &Snapshot) -> Vec<RelDelta> {
    program
        .relations
        .iter()
        .map(|relation| {
            let Some(rows) = after.get(&relation.rel) else {
                return RelDelta {
                    rel: relation.rel.clone(),
                    add: vec![],
                    del: vec![],
                };
            };
            let (add, del) = multiset_diff(
                before.get(&relation.rel).map(Vec::as_slice).unwrap_or(&[]),
                rows,
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
    dirty: &mut TickDirty,
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
    if statements.is_empty() {
        return Ok(());
    }
    for arrival in arrivals {
        dirty.arm(program, seam, &arrival.rel)?;
    }
    let results = {
        let _scope = crate::trace::Scope::phase("arrivals");
        seam.batch(&statements).expect("ordered arrivals failed")
    };
    for (arrival, result) in arrivals.iter().zip(results) {
        dirty.mark(&arrival.rel, result.rows_affected);
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
        let _scope = crate::trace::Scope::phase("snapshot_pre");
        seam.execute_multiple(&statements.join(";\n"))
            .expect("ordered pre snapshot failed");
    }
}

/// A level runs when a rel it reads moved after that level last ran, or when
/// `force` says the level tables hold nothing yet.
fn recompute_levels(
    program: &GenProgram,
    seam: &SqliteSeam,
    plan: &OrderedPlan,
    dirty: &mut TickDirty,
    stamps: &mut [u64],
    force: bool,
) -> BoundaryResult<()> {
    if program.ordered_recursive_levels {
        // The fixpoint loop reaches every level, so the dirty set learns nothing
        // here: every head is armed and marked, as the pre-dirty path did.
        for statement in &program.levels {
            dirty.arm(program, seam, &statement.head_rel)?;
        }
        for statement in &program.levels {
            LEVEL_RECOMPUTES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let _scope = crate::trace::Scope::verb("recompute", &statement.head_rel, "ordered");
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
                let _scope = crate::trace::Scope::verb("recompute", &statement.head_rel, "ordered");
                for sql in &statement.recompute_insert_sqls {
                    seam.execute_multiple(sql)
                        .expect("ordered recursive level insert failed");
                }
            }
            let rows = {
                let _scope = crate::trace::Scope::phase("recompute_round");
                seam.scalar(&count_sql)
                    .expect("ordered recursive level count failed")
            };
            if rows == prior {
                break;
            }
            prior = rows;
        }
        for statement in &program.levels {
            dirty.mark(&statement.head_rel, 1);
        }
        return Ok(());
    }
    for (index, statement) in program.levels.iter().enumerate() {
        if !force
            && !plan.level_always[index]
            && dirty.newest(program, &plan.level_reads[index]) <= stamps[index]
        {
            continue;
        }
        LEVEL_RECOMPUTES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        dirty.arm(program, seam, &statement.head_rel)?;
        let _scope = crate::trace::Scope::verb("recompute", &statement.head_rel, "ordered");
        let deleted = seam
            .execute(&SqlStatement {
                sql: statement.recompute_delete_sql.clone(),
                args: vec![],
            })
            .expect("ordered level clear failed")
            .rows_affected;
        let mut inserted = 0;
        for (arm, sql) in statement.recompute_insert_sqls.iter().enumerate() {
            let result = seam
                .execute(&SqlStatement {
                    sql: sql.clone(),
                    args: vec![],
                })
                .expect("ordered level recompute failed");
            if plan.level_head_inserts[index].contains(&arm) {
                inserted += result.rows_affected;
            }
        }
        stamps[index] = dirty.generation;
        // Equal non-zero counts say nothing about which rows they are, so that
        // one case pays a read rather than a false mark.
        let moved = match (deleted, inserted) {
            (0, 0) => false,
            (cleared, written) if cleared != written => true,
            _ => {
                let rows = read_relation(
                    program,
                    seam,
                    relation_of(program, &statement.head_rel),
                    false,
                )?;
                let (add, del) = multiset_diff(
                    dirty
                        .stored
                        .get(&statement.head_rel)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]),
                    &rows,
                );
                !add.is_empty() || !del.is_empty()
            }
        };
        if moved {
            dirty.mark(&statement.head_rel, 1);
        }
    }
    Ok(())
}

fn apply_retention(
    program: &GenProgram,
    seam: &SqliteSeam,
    dirty: &mut TickDirty,
) -> BoundaryResult<()> {
    let statements = program
        .retentions
        .iter()
        .map(|retention| SqlStatement {
            sql: retention.delete_sql.clone(),
            args: vec![],
        })
        .collect::<Vec<_>>();
    if statements.is_empty() {
        return Ok(());
    }
    for retention in &program.retentions {
        dirty.arm(program, seam, &retention.rel)?;
    }
    let results = {
        let _scope = crate::trace::Scope::verb("clear", "-", "ordered");
        seam.batch(&statements).expect("ordered retention failed")
    };
    for (retention, result) in program.retentions.iter().zip(results) {
        dirty.mark(&retention.rel, result.rows_affected);
    }
    Ok(())
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

/// Nothing but the staging writes a frontier table and nothing but a rel's own
/// writer fills it, so one reading at the top of the tick holds all tick.
fn read_probe(program: &GenProgram, seam: &SqliteSeam, plan: &OrderedPlan) -> TickProbe {
    let _scope = crate::trace::Scope::phase("probe");
    let mut answers: Vec<i64> = Vec::new();
    for probe in &plan.probe {
        let result = seam
            .execute(&SqlStatement {
                sql: probe.clone(),
                args: vec![],
            })
            .expect("ordered tick probe failed");
        let row = result.rows.first().expect("ordered tick probe row");
        answers.extend(row.iter().map(|value| value.as_i64().unwrap_or(0)));
    }
    let count = program.relations.len();
    let mut live = HashSet::new();
    let mut empty = HashSet::new();
    for (index, relation) in program.relations.iter().enumerate() {
        if answers[index] != 0 {
            live.insert(relation.rel.clone());
        }
        if answers[count + index] == 0 {
            empty.insert(relation.rel.clone());
        }
    }
    TickProbe { live, empty }
}

/// A TEMP table lives in the connection's own schema and dies with it, so its
/// absence is this process's first tick against this db.
fn first_fold(program: &GenProgram, seam: &SqliteSeam) -> bool {
    let _scope = crate::trace::Scope::phase("probe");
    let table = quote_identifier(&format!("__ordered_folded_{}", program.name));
    seam.execute(&SqlStatement {
        sql: format!("CREATE TEMP TABLE IF NOT EXISTS {table} (\"folded\" INTEGER PRIMARY KEY)"),
        args: vec![],
    })
    .expect("ordered fold marker failed");
    let result = seam
        .execute(&SqlStatement {
            sql: format!(
                "INSERT INTO temp.{table} (\"folded\") VALUES (1) \
                 ON CONFLICT DO NOTHING RETURNING \"folded\""
            ),
            args: vec![],
        })
        .expect("ordered fold marker read failed");
    !result.rows.is_empty()
}

fn count_rows(seam: &SqliteSeam, relation: &IncrementalRelationPlan) -> usize {
    let _scope = crate::trace::Scope::verb("snapshot", &relation.rel, "ordered");
    let result = seam
        .execute(&SqlStatement {
            sql: format!(
                "SELECT count(*) FROM {}",
                quote_identifier(&relation.table_name)
            ),
            args: vec![],
        })
        .expect("ordered row count failed");
    result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(|value| value.as_i64())
        .unwrap_or(0) as usize
}

fn read_carry(program: &GenProgram, seam: &SqliteSeam, live: &HashSet<String>) -> Vec<Occurrence> {
    let mut occurrences = Vec::new();
    for name in trigger_relations(program, OrderedTriggerKind::Arrival) {
        if !live.contains(&name) {
            continue;
        }
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
        let _scope = crate::trace::Scope::verb("read_staged", &name, "ordered");
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
        let column_indices: Vec<usize> = relation
            .columns
            .iter()
            .map(|column| {
                DIFF_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                column_index(&result, column).expect("carry column missing")
            })
            .collect();
        for row in &result.rows {
            let values = column_indices
                .iter()
                .map(|index| row[*index].clone())
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

fn read_departures(
    program: &GenProgram,
    seam: &SqliteSeam,
    live: &HashSet<String>,
) -> BoundaryResult<Vec<Occurrence>> {
    let mut occurrences = Vec::new();
    for name in trigger_relations(program, OrderedTriggerKind::Departure) {
        if !live.contains(&name) {
            continue;
        }
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
        let _scope = crate::trace::Scope::verb("read_staged", &name, "ordered");
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
    let triggers: HashMap<&str, &crate::types::IncrementalRelationPlan> = trigger_names
        .iter()
        .map(|name| {
            let relation = program
                .relations
                .iter()
                .find(|relation| relation.rel == *name)
                .expect("ordered trigger relation missing");
            (name.as_str(), relation)
        })
        .collect();
    let mut seen_by_rel: HashMap<&str, std::collections::HashSet<String>> = HashMap::new();
    for (name, relation) in &triggers {
        if relation.kind == RelationKind::Set {
            seen_by_rel.insert(
                name,
                before
                    .get(*name)
                    .map(|rows| {
                        rows.iter()
                            .map(|row| crate::incremental::dedup_key(row))
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
    }
    let mut occurrences = Vec::new();
    for arrival in arrivals {
        if arrival.sign != ArrivalSign::Add {
            continue;
        }
        let Some(relation) = triggers.get(arrival.rel.as_str()) else {
            continue;
        };
        if relation.kind == RelationKind::Set {
            let seen = seen_by_rel.entry(&arrival.rel).or_default();
            if !seen.insert(crate::incremental::dedup_key(&arrival.row)) {
                continue;
            }
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
    dirty: &mut TickDirty,
) -> BoundaryResult<()> {
    let _scope = crate::trace::Scope::verb("edge_write", &occurrence.rel, "ordered");
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
        let column_indices: Vec<usize> = arm
            .head_columns
            .iter()
            .map(|column| column_index(&result, column).expect("ordered head column missing"))
            .collect();
        for result_row in &result.rows {
            let row = column_indices
                .iter()
                .map(|index| result_row[*index].clone())
                .collect::<Row>();
            projected.push(OrderedWrite { arm_index, row });
        }
    }
    let mut writes: Vec<OrderedWrite> = Vec::new();
    let mut written_keys: std::collections::HashSet<(&str, String)> =
        std::collections::HashSet::new();
    let mut keyed: HashMap<String, Row> = HashMap::new();
    for write in projected {
        let arm = &program.ordered_arms[write.arm_index];
        DIFF_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if !written_keys.insert((
            arm.head_rel.as_str(),
            crate::incremental::dedup_key(&write.row),
        )) {
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
            match keyed.get(&key) {
                Some(prior) => {
                    if prior != &write.row {
                        panic!("keyed conflict in ordered occurrence for {}", arm.head_rel);
                    }
                }
                None => {
                    keyed.insert(key, write.row.clone());
                }
            }
        }
        writes.push(write);
    }
    let mut statements = Vec::new();
    // The pre arm writes a `__pre_` table, which is no rel's rows, so only the
    // head write carries a rel into the dirty set.
    let mut wrote: Vec<Option<&str>> = Vec::new();
    for write in &writes {
        let arm = &program.ordered_arms[write.arm_index];
        let args = ScalarValue::row_at_seam(&write.row, ScalarSeam::SqlParameter)?;
        statements.push(SqlStatement {
            sql: arm.write_sql.clone(),
            args: args.clone(),
        });
        wrote.push(Some(arm.head_rel.as_str()));
        if let Some(pre) = pre_write_statement(arm, &args) {
            statements.push(pre);
            wrote.push(None);
        }
    }
    if !statements.is_empty() {
        for write in &writes {
            dirty.arm(
                program,
                seam,
                &program.ordered_arms[write.arm_index].head_rel,
            )?;
        }
        let results = seam.batch(&statements).expect("ordered writes failed");
        let marks: Vec<(String, i64)> = wrote
            .iter()
            .zip(&results)
            .filter_map(|(rel, result)| rel.map(|rel| (rel.to_string(), result.rows_affected)))
            .collect();
        for (rel, rows_changed) in marks {
            dirty.mark(&rel, rows_changed);
        }
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
    let visible_adds: HashMap<&str, std::collections::HashSet<String>> = boundary
        .iter()
        .map(|delta| {
            (
                delta.rel.as_str(),
                delta
                    .add
                    .iter()
                    .map(|row| crate::incremental::dedup_key(row))
                    .collect(),
            )
        })
        .collect();
    let mut additions = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for write in written {
        let arm = &program.ordered_arms[write.arm_index];
        let key = crate::incremental::dedup_key(&write.row);
        let visible = visible_adds
            .get(arm.head_rel.as_str())
            .map(|adds| adds.contains(&key))
            .unwrap_or(false);
        if !visible || !seen.insert((arm.head_rel.clone(), key)) {
            continue;
        }
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
            let key = crate::incremental::dedup_key(&row);
            let visible = visible_adds
                .get(name.as_str())
                .map(|adds| adds.contains(&key))
                .unwrap_or(false);
            if !visible || !seen.insert((name.clone(), key)) {
                continue;
            }
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
    let plan = ordered_plan(program);
    let probe = read_probe(program, seam, &plan);
    let mut dirty = TickDirty::new(probe.empty);
    let mut stamps = vec![0u64; program.levels.len()];
    let live = probe.live;
    // A db this process has not folded before may carry level tables a killed
    // process left inconsistent with their sources, so the first tick rebuilds.
    let force = first_fold(program, seam);
    // Armed before the clock turns, which is where the pre-dirty path read the
    // whole before-snapshot.
    for arrival in arrivals {
        dirty.arm(program, seam, &arrival.rel)?;
    }
    // The struct plane interns its own rows through its own apply_arrivals, so
    // the rel it writes is armed here and read back below.
    let interning: Vec<String> = arrivals
        .iter()
        .any(|arrival| program.struct_ref_columns.contains_key(&arrival.rel))
        .then(|| plan.struct_rels.clone())
        .unwrap_or_default();
    for rel in &interning {
        dirty.arm(program, seam, rel)?;
    }
    if program.uses_tick {
        crate::incremental::advance_tick(seam);
    }
    let interning_scope = crate::trace::Scope::phase("intern");
    let enumed = crate::enum_plane::intern(
        seam,
        &program.enum_types,
        &program.enum_ref_columns,
        std::borrow::Cow::Borrowed(arrivals),
    )?;
    let interned = match &program.text_intern_plan {
        Some(plan) => crate::text_plane::intern(seam, plan, enumed)?,
        None => enumed,
    };
    let normalized = crate::struct_plane::intern(
        seam,
        &program.struct_types,
        &program.struct_ref_columns,
        interned,
        &program.relations,
        program.text_intern_plan.as_ref(),
    )?;
    drop(interning_scope);
    for rel in &interning {
        let held = dirty.stored.get(rel).map(Vec::len).unwrap_or(0);
        // The intern arm only ever adds rows, so an unmoved count is an unmoved
        // table.
        if count_rows(seam, relation_of(program, rel)) != held {
            dirty.mark(rel, 1);
        }
    }
    apply_arrivals(program, seam, &normalized, &mut dirty)?;
    snapshot_pre(program, seam);
    recompute_levels(program, seam, &plan, &mut dirty, &mut stamps, force)?;
    let mut mid = read_snapshot_of(program, seam, false, &dirty.moved_within(&plan.level_heads))?;
    let mut occurrences = read_carry(program, seam, &live);
    occurrences.extend(read_departures(program, seam, &live)?);
    occurrences.extend(outside_occurrences(program, &dirty.stored, &normalized));
    occurrences.extend(level_occurrences(program, &dirty.stored, &mid));
    let mut written = Vec::new();
    for occurrence in &occurrences {
        apply_occurrence(program, seam, occurrence, &mut written, &mut dirty)?;
    }
    recompute_levels(program, seam, &plan, &mut dirty, &mut stamps, false)?;
    apply_retention(program, seam, &mut dirty)?;
    let moved = dirty.moved();
    let after_decoded = read_snapshot_of(program, seam, true, &moved)?;
    // The stored diff answers `carry_additions`, which asks about arm heads and
    // level heads only.
    let after_stored =
        read_snapshot_of(program, seam, false, &dirty.moved_within(&plan.carry_rels))?;
    let deltas = build_deltas(program, &dirty.decoded, &after_decoded);
    let stored_deltas = build_deltas(program, &dirty.stored, &after_stored);
    // A rel first written after `mid` was read still holds its armed rows there:
    // nothing wrote it in between.
    for rel in &moved {
        if let Some(rows) = dirty.stored.get(rel) {
            mid.entry(rel.clone()).or_insert_with(|| rows.clone());
        }
    }
    let additions = carry_additions(program, &mid, &after_stored, &stored_deltas, &written);
    let staged: HashSet<&str> = additions.iter().map(|delta| delta.rel.as_str()).collect();
    // Only a rel whose frontier holds rows needs clearing, and only one with
    // additions needs staging; the rest are empty tables.
    let staging: Vec<IncrementalRelationPlan> = program
        .relations
        .iter()
        .filter(|relation| live.contains(&relation.rel) || staged.contains(relation.rel.as_str()))
        .cloned()
        .collect();
    let ordered_carry = {
        let _scope = crate::trace::Scope::phase("stage_carry");
        crate::incremental::stage_ordered_frontiers(seam, &staging, &additions)?
    };
    // Decoded rows, not stored ids: read_departures types the frontier table
    // back through the rel's declared column types, and the TypeScript door
    // stages the same decoded delta (emit_ts.pl snapshot_departure_stage_lines).
    crate::incremental::stage_departures(seam, &program.relations, &deltas)?;
    Ok(TickDeltas {
        carry_pending: base_carry_pending(program, &deltas) || ordered_carry,
        rels: deltas,
    })
}
