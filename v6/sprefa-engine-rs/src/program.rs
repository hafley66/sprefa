// The generated-program contract: IGenProgram's Rust equivalent. The five
// pinned fields are emitter-stable ("extend by adding fields, never renaming")
// and `boot` and `final_select` are extra fields exactly as in the TS door.

use std::collections::HashMap;
use std::pin::Pin;

use futures::stream;
use futures::Stream;

use crate::incremental;
use crate::sql::{SqlRunner, SqliteSeam};
use crate::types::{
    Arrival, ArrivalTemplate, BootStatement, BoundaryResult, IncrementalEdgeStatement,
    IncrementalLevelStatement, IncrementalRelationPlan, IncrementalRetentionStatement, InternMode,
    ProgramJson, TickDeltas,
};
#[derive(Clone)]
pub struct GenProgram {
    pub name: String,
    pub intern_mode: InternMode,
    pub ddl: Vec<String>,
    pub rel_columns: HashMap<String, Vec<String>>,
    pub rel_column_types: HashMap<String, Vec<crate::types::RowColumnType>>,
    pub arrival_targets: Vec<String>,
    pub boot: Vec<BootStatement>,
    pub final_select: HashMap<String, String>,
    pub arrival_templates: HashMap<String, ArrivalTemplate>,
    pub text_intern_plan: Option<crate::types::TextInternPlan>,
    pub struct_types: Vec<crate::types::StructTypePlan>,
    pub struct_ref_columns: HashMap<String, Vec<Option<String>>>,
    pub enum_types: Vec<crate::types::EnumTypePlan>,
    pub enum_ref_columns: crate::types::EnumRefColumns,
    pub pre_snapshot_rels: Vec<String>,
    pub relations: Vec<IncrementalRelationPlan>,
    pub edges: Vec<IncrementalEdgeStatement>,
    pub levels: Vec<IncrementalLevelStatement>,
    pub retentions: Vec<IncrementalRetentionStatement>,
    pub uses_tick: bool,
    pub reconcile_every_tick: bool,
    pub incremental_safe: bool,
    pub ir_version: u32,
    pub host_plans: Vec<crate::types::HostPlanData>,
    pub queries: Vec<String>,
    /// Which level heads feed another round, decided once here because the
    /// answer is a substring pass over every insert text.
    pub recursive_level_heads: Vec<String>,
    /// Per level head: the rels it reads, decided once because the answer is a
    /// substring pass over every emitted text.
    pub level_sources: HashMap<String, incremental::LevelSources>,
}

// The IR shape this runtime interprets. Bumped whenever a field's meaning
// moves; emit_rust.pl and emit_ts.pl carry the same number as `ir_version/1`.
pub const IR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrVersionMismatch {
    pub program: String,
    pub found: u32,
    pub expected: u32,
}

impl std::fmt::Display for IrVersionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ir_version_mismatch: program {} was emitted at ir_version {} and this runtime interprets {}",
            self.program, self.found, self.expected
        )
    }
}

impl std::error::Error for IrVersionMismatch {}

impl GenProgram {
    // The checked door. `from_json` is the panicking wrapper every in-tree
    // harness already calls; a binary that boots an IR it did not build reads
    // the mismatch as a value.
    pub fn try_from_json(pj: ProgramJson) -> Result<Self, IrVersionMismatch> {
        if pj.ir_version != IR_VERSION {
            return Err(IrVersionMismatch {
                program: pj.name,
                found: pj.ir_version,
                expected: IR_VERSION,
            });
        }
        Ok(GenProgram::from_checked_json(pj))
    }

    pub fn from_json(pj: ProgramJson) -> Self {
        match GenProgram::try_from_json(pj) {
            Ok(program) => program,
            Err(mismatch) => panic!("{mismatch}"),
        }
    }

    fn from_checked_json(pj: ProgramJson) -> Self {
        let recursive_level_heads = incremental::recursive_heads(&pj.levels, &pj.relations);
        let level_sources =
            incremental::level_sources(&pj.levels, &pj.relations, &recursive_level_heads);
        GenProgram {
            name: pj.name,
            intern_mode: pj.intern_mode,
            ddl: pj.ddl,
            rel_columns: pj.rel_columns,
            rel_column_types: pj.rel_column_types,
            arrival_targets: pj.arrival_targets,
            boot: pj.boot,
            final_select: pj.final_select,
            arrival_templates: pj.arrival_templates,
            text_intern_plan: pj.text_intern_plan,
            struct_types: pj.struct_types,
            struct_ref_columns: pj.struct_ref_columns,
            enum_types: pj.enum_types,
            enum_ref_columns: pj.enum_ref_columns,
            pre_snapshot_rels: pj.pre_snapshot_rels,
            relations: pj.relations,
            edges: pj.edges,
            levels: pj.levels,
            retentions: pj.retentions,
            uses_tick: pj.uses_tick,
            reconcile_every_tick: pj.reconcile_every_tick,
            incremental_safe: pj.incremental_safe,
            ir_version: pj.ir_version,
            host_plans: pj.host_plans,
            queries: pj.queries,
            recursive_level_heads,
            level_sources,
        }
    }

    /// The statement texts the IR fixes ahead of the fold; a row-count-shaped
    /// VALUES list is not among them. The cache size that evicts nothing.
    pub fn stable_sql_count(&self) -> usize {
        let mut count = self.ddl.len() + self.boot.len() + self.final_select.len();
        for relation in &self.relations {
            count += 1
                + usize::from(relation.arrival_add_sql.is_some())
                + usize::from(relation.arrival_del_sql.is_some());
        }
        for edge in &self.edges {
            count += 1 + edge.intern_sql.as_ref().map(|sqls| sqls.len()).unwrap_or(0);
        }
        for level in &self.levels {
            count += 3 + level.recompute_insert_sqls.len();
            count += level
                .intern_sql
                .as_ref()
                .map(|sqls| sqls.len())
                .unwrap_or(0);
            count += level
                .support_sql
                .as_ref()
                .map(|sqls| sqls.len())
                .unwrap_or(0);
            count += level
                .support_intern_sql
                .as_ref()
                .map(|sqls| sqls.len())
                .unwrap_or(0);
            if let Some(plan) = &level.support_count_sql {
                count += 1 + plan.write_sqls.len();
            }
            if let Some(expand) = &level.expand_sql {
                count += 7 + expand.seed_sqls.len();
            }
            if let Some(aggregate) = &level.aggregate_sql {
                count += 2
                    + aggregate.scope_seed_sql.len()
                    + aggregate.insert_scoped_sql.len()
                    + aggregate
                        .intern_sql
                        .as_ref()
                        .map(|sqls| sqls.len())
                        .unwrap_or(0);
            }
        }
        count += self.retentions.len();
        count
    }

    pub fn tick<'a>(
        &'a self,
        seam: &'a SqliteSeam,
        arrivals: Vec<Arrival>,
    ) -> Pin<Box<dyn Stream<Item = BoundaryResult<TickDeltas>> + 'a>> {
        Box::pin(stream::once(async move { self.run_tick(seam, &arrivals) }))
    }

    pub fn run_tick(&self, seam: &SqliteSeam, arrivals: &[Arrival]) -> BoundaryResult<TickDeltas> {
        let work = incremental::TickWork::probe(seam, &self.relations);
        incremental::prepare_tick(seam, &self.relations, &work);
        if self.uses_tick {
            incremental::advance_tick(seam);
        }
        let normalized = {
            let _scope = crate::trace::Scope::phase("intern");
            let enumed = crate::enum_plane::intern(
                seam,
                &self.enum_types,
                &self.enum_ref_columns,
                std::borrow::Cow::Borrowed(arrivals),
            )?;
            let interned = match &self.text_intern_plan {
                Some(plan) => crate::text_plane::intern(seam, plan, enumed)?,
                None => enumed,
            };
            crate::struct_plane::intern(
                seam,
                &self.struct_types,
                &self.struct_ref_columns,
                interned,
                &self.relations,
                self.text_intern_plan.as_ref(),
                &work,
            )?
        };
        let arrivals = normalized.as_ref();
        incremental::apply_arrivals(seam, arrivals, &self.relations, &work)?;
        // Before the level phase: a `pre/1` body over a level head reads that
        // head as the previous tick settled it.
        incremental::snapshot_pre(seam, &self.pre_snapshot_rels, &self.relations)?;
        incremental::apply_levels_before_edges(
            seam,
            &self.levels,
            &self.relations,
            &self.recursive_level_heads,
            &work,
            &self.level_sources,
        )?;
        if !self.edges.is_empty() {
            incremental::recompute_levels_before_edges(
                seam,
                &self.levels,
                &self.relations,
                self.reconcile_every_tick,
                arrivals.len(),
                &work,
                &self.level_sources,
            )?;
            incremental::apply_edges(seam, &self.edges, &self.relations, &work)?;
            incremental::merge_next_into_current(seam, &self.relations, &work);
            incremental::apply_levels_after_edges(
                seam,
                &self.levels,
                &self.relations,
                &work,
                &self.level_sources,
            )?;
        }
        incremental::apply_retention(seam, &self.retentions, &self.relations, &work)?;
        incremental::recompute_levels_after_edges(
            seam,
            &self.levels,
            &self.relations,
            self.reconcile_every_tick,
            &work,
            &self.level_sources,
        )?;
        let physical_rels = incremental::read_boundary(seam, &self.relations, &work)?;
        incremental::stage_departures(seam, &self.relations, &physical_rels, &work)?;
        let rels = {
            let _scope = crate::trace::Scope::phase("decode");
            crate::enum_plane::decode_deltas(
                seam,
                &self.enum_types,
                &self.enum_ref_columns,
                &self.relations,
                physical_rels,
            )?
        };
        let carry_pending = incremental::promote_frontiers(seam, &self.relations, &work);
        Ok(TickDeltas {
            rels,
            carry_pending,
        })
    }
}

// run_boot executes the emitted boot statements after DDL and before the tick
// fold. Statements with params bind through the seam; bare statements run as
// possibly multi-statement text.
pub fn run_boot(seam: &SqliteSeam, statements: &[BootStatement]) {
    let _scope = crate::trace::Scope::phase("boot");
    for statement in statements {
        if statement.params.is_empty() {
            seam.execute_multiple(&statement.sql)
                .expect("boot statement failed");
        } else {
            let _ = seam.execute(&crate::types::SqlStatement {
                sql: statement.sql.clone(),
                args: statement.params.clone(),
            });
        }
    }
}
