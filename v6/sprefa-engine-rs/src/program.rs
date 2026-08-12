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
    Arrival, ArrivalTemplate, BootStatement, IncrementalEdgeStatement, IncrementalLevelStatement,
    IncrementalRelationPlan, IncrementalRetentionStatement, InternMode, ProgramJson, TickDeltas,
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
    pub relations: Vec<IncrementalRelationPlan>,
    pub edges: Vec<IncrementalEdgeStatement>,
    pub levels: Vec<IncrementalLevelStatement>,
    pub retentions: Vec<IncrementalRetentionStatement>,
    pub reconcile_every_tick: bool,
    pub incremental_safe: bool,
}

impl GenProgram {
    pub fn from_json(pj: ProgramJson) -> Self {
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
            relations: pj.relations,
            edges: pj.edges,
            levels: pj.levels,
            retentions: pj.retentions,
            reconcile_every_tick: pj.reconcile_every_tick,
            incremental_safe: pj.incremental_safe,
        }
    }

    pub fn tick<'a>(
        &'a self,
        seam: &'a SqliteSeam,
        arrivals: Vec<Arrival>,
    ) -> Pin<Box<dyn Stream<Item = TickDeltas> + 'a>> {
        Box::pin(stream::once(async move { self.run_tick(seam, &arrivals) }))
    }

    pub fn run_tick(&self, seam: &SqliteSeam, arrivals: &[Arrival]) -> TickDeltas {
        incremental::prepare_tick(seam, &self.relations);
        let interned = match &self.text_intern_plan {
            Some(plan) => crate::text_plane::intern(seam, plan, arrivals),
            None => arrivals.to_vec(),
        };
        let arrivals = interned.as_slice();
        incremental::apply_arrivals(seam, arrivals, &self.relations);
        incremental::apply_levels_before_edges(seam, &self.levels, &self.relations);
        if !self.edges.is_empty() {
            incremental::recompute_levels_before_edges(
                seam,
                &self.levels,
                &self.relations,
                self.reconcile_every_tick,
                arrivals.len(),
            );
            incremental::apply_edges(seam, &self.edges, &self.relations);
            incremental::merge_next_into_current(seam, &self.relations);
            incremental::apply_levels_after_edges(seam, &self.levels, &self.relations);
        }
        incremental::recompute_levels_after_edges(
            seam,
            &self.levels,
            &self.relations,
            self.reconcile_every_tick,
        );
        let rels = incremental::read_boundary(seam, &self.relations);
        let carry_pending = incremental::promote_frontiers(seam, &self.relations);
        TickDeltas {
            rels,
            carry_pending,
        }
    }
}

// run_boot executes the emitted boot statements after DDL and before the tick
// fold. Statements with params bind through the seam; bare statements run as
// possibly multi-statement text.
pub fn run_boot(seam: &SqliteSeam, statements: &[BootStatement]) {
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
