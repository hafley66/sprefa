// The six write verbs of a tick (arrive, stage, read_staged, recount,
// publish, clear) and the two transient-storage strategies behind them, the
// mirror of v6/tsv2/runtime/writeVerbs.ts.
//
// per_rel writes each rel's own __delta_/__frontier_/__next_frontier_ tables.
// shared writes one __frontier, one __next_frontier and one __support_count,
// every row carrying its relation_id
// (plans/2026-08-19-shared-sqlite-frontier.md). A rel's own frontier names
// survive as TEMP views under shared, so every compiled read keeps its text.
//
// write_verbs_for reads the strategy off the plan metadata; no statement
// inside a tick loop asks which mode it is in.

use crate::incremental::{
    boundary_stage_statement, direct_arrival_statement, frontier_stage_statement, quote_identifier,
    DeltaEvent,
};
use crate::types::{
    BoundaryResult, IncrementalLevelStatement, IncrementalRelationPlan, Row, SqlStatement,
};

pub const SHARED_FRONTIER_TABLE: &str = "__frontier";
pub const SHARED_NEXT_FRONTIER_TABLE: &str = "__next_frontier";
pub const SHARED_SUPPORT_TABLE: &str = "__support_count";

const SHARED_FRONTIER_COLUMNS: &str = "\"relation_id\", \"_phase\", \"_sequence\", \"row_id\"";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteVerbStrategy {
    PerRel,
    Shared,
}

/// Where a tick stands when it calls `clear`. One variant per TABLE the
/// boundary touches, so a rel whose frontier held no row pays for none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickBoundary {
    PrepareDelta,
    PrepareNext,
    Merge,
    PromoteDrop,
    PromoteMove,
}

pub trait WriteVerbs: Sync {
    fn strategy(&self) -> WriteVerbStrategy;
    /// rel + rows + sign -> the durable typed write.
    fn arrive(
        &self,
        relation: &IncrementalRelationPlan,
        sign: i8,
        rows: &[&Row],
    ) -> BoundaryResult<SqlStatement>;
    /// rel + rows -> the boundary delta write, plus one frontier write per
    /// copy when the batch carries additions.
    fn stage(
        &self,
        relation: &IncrementalRelationPlan,
        events: &[&DeltaEvent],
        copies: &[(String, i64)],
    ) -> BoundaryResult<Vec<SqlStatement>>;
    /// The staged-row read a tick boundary asks for carry. Empty text when
    /// there is nothing staged to ask about.
    fn read_staged(&self, relations: &[IncrementalRelationPlan]) -> String;
    /// rule -> the statements closing a recount round after the head insert.
    fn recount(&self, statement: &IncrementalLevelStatement) -> Vec<String>;
    /// rel -> the boundary delta read.
    fn publish(&self, relation: &IncrementalRelationPlan) -> SqlStatement;
    /// tick boundary -> the transient state it empties or moves.
    fn clear(&self, relations: &[IncrementalRelationPlan], boundary: TickBoundary) -> Vec<String>;
}

impl WriteVerbStrategy {
    pub fn name(self) -> &'static str {
        match self {
            WriteVerbStrategy::PerRel => "per_rel",
            WriteVerbStrategy::Shared => "shared",
        }
    }
}

/// The strategy name a span carries, read off plan metadata like the verbs
/// themselves.
pub fn strategy_name(relations: &[IncrementalRelationPlan]) -> &'static str {
    write_verbs_for(relations).strategy().name()
}

pub fn relation_strategy(relation: &IncrementalRelationPlan) -> &'static str {
    if relation.shared_frontier.is_some() {
        WriteVerbStrategy::Shared.name()
    } else {
        WriteVerbStrategy::PerRel.name()
    }
}

pub struct PerRelWriteVerbs;
pub struct SharedWriteVerbs;

static PER_REL: PerRelWriteVerbs = PerRelWriteVerbs;
static SHARED: SharedWriteVerbs = SharedWriteVerbs;

/// One resolution per relations slice: the strategy is metadata, constant for
/// the life of a program.
pub fn write_verbs_for(relations: &[IncrementalRelationPlan]) -> &'static dyn WriteVerbs {
    if relations.iter().any(|r| r.shared_frontier.is_some()) {
        &SHARED
    } else {
        &PER_REL
    }
}

/// arrive and publish read the durable and boundary planes, which no strategy
/// changes.
fn durable_arrive(
    relation: &IncrementalRelationPlan,
    sign: i8,
    rows: &[&Row],
) -> BoundaryResult<SqlStatement> {
    if relation
        .column_types
        .contains(&crate::types::RowColumnType::Bytes)
    {
        return direct_arrival_statement(relation, sign, rows);
    }
    let sql = if sign == 1 {
        relation
            .arrival_add_sql
            .clone()
            .expect("incremental add statement missing")
    } else {
        relation
            .arrival_del_sql
            .clone()
            .expect("incremental delete statement missing")
    };
    let mut encoded = String::new();
    encoded.push('[');
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            encoded.push(',');
        }
        encoded.push_str(&crate::incremental::json_array_text(row)?);
    }
    encoded.push(']');
    Ok(SqlStatement {
        sql,
        args: vec![crate::types::ScalarValue::Text(encoded)],
    })
}

fn durable_publish(relation: &IncrementalRelationPlan) -> SqlStatement {
    SqlStatement {
        sql: relation.boundary_sql.clone(),
        args: vec![],
    }
}

fn staged_statements(
    relation: &IncrementalRelationPlan,
    events: &[&DeltaEvent],
    copies: &[(String, i64)],
) -> BoundaryResult<Vec<SqlStatement>> {
    let mut statements = vec![boundary_stage_statement(relation, events)?];
    let additions: Vec<&DeltaEvent> = events.iter().filter(|e| e.sign == 1).copied().collect();
    if additions.is_empty() {
        return Ok(statements);
    }
    for (table_name, phase) in copies {
        statements.push(frontier_stage_statement(
            relation, table_name, *phase, &additions,
        )?);
    }
    Ok(statements)
}

/// A staged departure is carry the way a staged addition is: engine.pl
/// appends DepartureCarry to ArrivalCarry in one CarryOut list.
fn departure_terms(relation: &IncrementalRelationPlan, terms: &mut Vec<String>) {
    if let Some(table_name) = &relation.departure_frontier_table_name {
        terms.push(format!(
            "EXISTS (SELECT 1 FROM {} LIMIT 1)",
            quote_identifier(table_name)
        ));
    }
}

fn carry_probe(terms: &[String]) -> String {
    if terms.is_empty() {
        return String::new();
    }
    format!(
        "SELECT CASE WHEN {} THEN 1 ELSE 0 END AS carry_pending",
        terms.join(" OR ")
    )
}

fn frontier_columns_text(relation: &IncrementalRelationPlan) -> String {
    let mut columns = vec!["_phase".to_string(), "_sequence".to_string()];
    columns.extend(relation.columns.clone());
    columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<String>>()
        .join(", ")
}

impl WriteVerbs for PerRelWriteVerbs {
    fn strategy(&self) -> WriteVerbStrategy {
        WriteVerbStrategy::PerRel
    }

    fn arrive(
        &self,
        relation: &IncrementalRelationPlan,
        sign: i8,
        rows: &[&Row],
    ) -> BoundaryResult<SqlStatement> {
        durable_arrive(relation, sign, rows)
    }

    fn stage(
        &self,
        relation: &IncrementalRelationPlan,
        events: &[&DeltaEvent],
        copies: &[(String, i64)],
    ) -> BoundaryResult<Vec<SqlStatement>> {
        staged_statements(relation, events, copies)
    }

    fn read_staged(&self, relations: &[IncrementalRelationPlan]) -> String {
        let mut terms = Vec::new();
        for relation in relations {
            terms.push(format!(
                "EXISTS (SELECT 1 FROM {} LIMIT 1)",
                quote_identifier(&relation.next_frontier_table_name)
            ));
            departure_terms(relation, &mut terms);
        }
        carry_probe(&terms)
    }

    fn recount(&self, _statement: &IncrementalLevelStatement) -> Vec<String> {
        Vec::new()
    }

    fn publish(&self, relation: &IncrementalRelationPlan) -> SqlStatement {
        durable_publish(relation)
    }

    fn clear(&self, relations: &[IncrementalRelationPlan], boundary: TickBoundary) -> Vec<String> {
        let mut statements = Vec::new();
        for relation in relations {
            let copy = || {
                let joined = frontier_columns_text(relation);
                format!(
                    "INSERT INTO {} ({}) SELECT {} FROM {}",
                    quote_identifier(&relation.frontier_table_name),
                    joined,
                    joined,
                    quote_identifier(&relation.next_frontier_table_name)
                )
            };
            match boundary {
                TickBoundary::PrepareDelta => statements.push(format!(
                    "DELETE FROM {}",
                    quote_identifier(&relation.delta_table_name)
                )),
                TickBoundary::PrepareNext => statements.push(format!(
                    "DELETE FROM {}",
                    quote_identifier(&relation.next_frontier_table_name)
                )),
                TickBoundary::Merge => statements.push(copy()),
                TickBoundary::PromoteDrop => statements.push(format!(
                    "DELETE FROM {}",
                    quote_identifier(&relation.frontier_table_name)
                )),
                TickBoundary::PromoteMove => {
                    statements.push(copy());
                    statements.push(format!(
                        "DELETE FROM {}",
                        quote_identifier(&relation.next_frontier_table_name)
                    ));
                }
            }
        }
        statements
    }
}

impl WriteVerbs for SharedWriteVerbs {
    fn strategy(&self) -> WriteVerbStrategy {
        WriteVerbStrategy::Shared
    }

    fn arrive(
        &self,
        relation: &IncrementalRelationPlan,
        sign: i8,
        rows: &[&Row],
    ) -> BoundaryResult<SqlStatement> {
        durable_arrive(relation, sign, rows)
    }

    fn stage(
        &self,
        relation: &IncrementalRelationPlan,
        events: &[&DeltaEvent],
        copies: &[(String, i64)],
    ) -> BoundaryResult<Vec<SqlStatement>> {
        staged_statements(relation, events, copies)
    }

    fn read_staged(&self, relations: &[IncrementalRelationPlan]) -> String {
        let mut terms = vec![format!(
            "EXISTS (SELECT 1 FROM {} LIMIT 1)",
            quote_identifier(SHARED_NEXT_FRONTIER_TABLE)
        )];
        for relation in relations {
            departure_terms(relation, &mut terms);
        }
        carry_probe(&terms)
    }

    fn recount(&self, statement: &IncrementalLevelStatement) -> Vec<String> {
        let Some(plan) = &statement.support_count_sql else {
            return Vec::new();
        };
        let mut statements = vec![plan.clear_sql.clone()];
        statements.extend(plan.write_sqls.clone());
        statements
    }

    fn publish(&self, relation: &IncrementalRelationPlan) -> SqlStatement {
        durable_publish(relation)
    }

    fn clear(&self, relations: &[IncrementalRelationPlan], boundary: TickBoundary) -> Vec<String> {
        let copy = format!(
            "INSERT INTO {} ({}) SELECT {} FROM {}",
            quote_identifier(SHARED_FRONTIER_TABLE),
            SHARED_FRONTIER_COLUMNS,
            SHARED_FRONTIER_COLUMNS,
            quote_identifier(SHARED_NEXT_FRONTIER_TABLE)
        );
        // The three shared tables are emptied whole: one rel's rows in them are
        // exactly the rows this tick's boundary set names.
        match boundary {
            TickBoundary::PrepareDelta => relations
                .iter()
                .map(|relation| {
                    format!(
                        "DELETE FROM {}",
                        quote_identifier(&relation.delta_table_name)
                    )
                })
                .collect(),
            TickBoundary::PrepareNext => vec![format!(
                "DELETE FROM {}",
                quote_identifier(SHARED_NEXT_FRONTIER_TABLE)
            )],
            TickBoundary::Merge => vec![copy],
            TickBoundary::PromoteDrop => vec![format!(
                "DELETE FROM {}",
                quote_identifier(SHARED_FRONTIER_TABLE)
            )],
            TickBoundary::PromoteMove => vec![
                copy,
                format!(
                    "DELETE FROM {}",
                    quote_identifier(SHARED_NEXT_FRONTIER_TABLE)
                ),
            ],
        }
    }
}
