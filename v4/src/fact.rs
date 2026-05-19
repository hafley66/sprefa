//! `FactWrite` / `FactRead` Components.
//!
//! The `FactStore` trait + Mem/Sqlite impls live in
//! `effect_runtime::v2::fact_store` now (unified with the old
//! `MutationStore`). This file holds the pipeline pieces that consume
//! it on the v4 side, plus convenient re-exports.

use std::sync::Arc;

use effect_runtime::v2::{Component, FactStore, Node, RenderCtx};

use crate::mounted_query;
use crate::runtime_graph::RuntimeGraph;
use crate::Cursor;

// Re-exports so existing call sites keep compiling.
pub use effect_runtime::v2::{FactStore as FactStoreTrait, MemFactStore, SqliteFactStore};

// ───────────────────────────────────────────────────────────────────
// FactWrite / FactRead Components
// ───────────────────────────────────────────────────────────────────

/// `fact(:name) > FactWrite { cols }`. Row-INSERT. Pass-through.
pub struct FactWrite {
    pub store: Arc<dyn FactStore<Cursor>>,
    pub table: Arc<str>,
    pub assignments: Option<Arc<Vec<WriteAssign>>>,
    /// True iff this write's pipeline re-derives its FULL extent every
    /// run (constant literal/stream head, no fs/read/ast/glob/… source).
    /// Owner-scoped cross-run retraction is sound ONLY then; an
    /// fs-driven (incremental, warm-sliced) write sets this false so a
    /// partial re-run never retracts an unchanged-file row. Set at
    /// lower time from the pipe's head ops; defaults true.
    pub full_extent: bool,
}

#[derive(Clone)]
pub enum WriteValue {
    Term(Arc<str>),
    Value,
    Literal(Arc<str>),
}

#[derive(Clone)]
pub struct WriteAssign {
    pub col: Arc<str>,
    pub value: WriteValue,
}

impl FactWrite {
    pub fn new(store: Arc<dyn FactStore<Cursor>>, table: impl Into<Arc<str>>) -> Self {
        Self {
            store,
            table: table.into(),
            assignments: None,
            full_extent: true,
        }
    }

    pub fn projected(
        store: Arc<dyn FactStore<Cursor>>,
        table: impl Into<Arc<str>>,
        assignments: Vec<WriteAssign>,
    ) -> Self {
        Self {
            store,
            table: table.into(),
            assignments: Some(Arc::new(assignments)),
            full_extent: true,
        }
    }

    /// Lower-time builder: mark whether the enclosing pipe re-derives
    /// its full extent each run (see `full_extent`).
    pub fn with_full_extent(mut self, full: bool) -> Self {
        self.full_extent = full;
        self
    }

    /// Stable identity of THIS write site for owner-scoped retraction:
    /// `(table, assignment shape)`. Two writers into one rule table
    /// (e.g. distinct `r!(...)` call sites with different literal args)
    /// fold to distinct owners, so a re-run reconcile never lets one
    /// owner retract another's rows. A term-projected write (e.g.
    /// `edge(X, Y)`) keeps a constant key across runs while its rows
    /// vary by input, so its prior extent reconciles correctly.
    fn owner_key(&self) -> String {
        let mut buf = String::with_capacity(64);
        buf.push_str(&self.table);
        if let Some(assignments) = &self.assignments {
            for a in assignments.iter() {
                buf.push('\u{1f}');
                buf.push_str(&a.col);
                buf.push('=');
                match &a.value {
                    WriteValue::Term(t) => {
                        buf.push('T');
                        buf.push_str(t);
                    }
                    WriteValue::Value => buf.push('V'),
                    WriteValue::Literal(l) => {
                        buf.push('L');
                        buf.push_str(l);
                    }
                }
            }
        }
        blake3::hash(buf.as_bytes()).to_hex().to_string()
    }
}

impl Component for FactWrite {
    type Next = Cursor;

    /// Batch path — ONE store lock per component invocation, not per
    /// cursor. The batch is also the splice unit, so no per-row Node::Emit
    /// wrapping; we hand back exactly what came in.
    fn render_batch(&self, ctx: &RenderCtx, batch: &[&Cursor]) -> Vec<Node<Cursor>> {
        if batch.is_empty() {
            return Vec::new();
        }
        let arced: Vec<Arc<Cursor>> = batch.iter().map(|c| Arc::new((*c).clone())).collect();
        let rows: Vec<Arc<Cursor>> = match &self.assignments {
            Some(assignments) => batch
                .iter()
                .map(|c| {
                    let mut row = Cursor::default();
                    for assignment in assignments.iter() {
                        match &assignment.value {
                            WriteValue::Term(term) => {
                                if let Some(value) = c.get(term) {
                                    row.set(&assignment.col, value);
                                }
                            }
                            WriteValue::Value => {
                                row.set_arc(&assignment.col, c.value.clone());
                            }
                            WriteValue::Literal(value) => {
                                row.set_arc(&assignment.col, value.clone());
                            }
                        }
                    }
                    Arc::new(row)
                })
                .collect(),
            None => arced.clone(),
        };
        let support_input_count = batch
            .iter()
            .filter(|cursor| cursor.get(mounted_query::SUPPORT_CURSOR_ID).is_some())
            .count();
        let support_rows: Vec<(String, String, String)> =
            if mounted_query::should_record_fact_support_count(support_input_count) {
                batch
                    .iter()
                    .zip(rows.iter())
                    .filter_map(|(cursor, row)| {
                        cursor
                            .get(mounted_query::SUPPORT_CURSOR_ID)
                            .map(|support_id| {
                                (
                                    support_id.to_string(),
                                    self.table.to_string(),
                                    self.store.row_id_for(self.table.as_ref(), row.as_ref()),
                                )
                            })
                    })
                    .collect()
            } else {
                Vec::new()
            };
        // No upstream stamped support ⇒ a stream/literal-fed rule write.
        // The mounted-query / fs-hits stamping never ran, so without
        // this the table is insert-only and a re-run that no longer
        // produces a row cannot retract it. Self-support each produced
        // row and owner-scoped reconcile against the prior run's set.
        let owner_reconcile_ids: Vec<String> = if support_input_count == 0 {
            rows.iter()
                .map(|row| self.store.row_id_for(self.table.as_ref(), row.as_ref()))
                .collect()
        } else {
            Vec::new()
        };
        self.store.insert_batch(&self.table, rows);
        if let Some(graph) = ctx.runtime::<RuntimeGraph>() {
            graph.notify_table_inserted(&self.table, ctx.expand_tick);
        }
        mounted_query::record_fact_support_batch(self.store.as_ref(), &support_rows);
        // Owner-scoped retraction is sound only when this write
        // re-derives its FULL extent each run (constant literal/stream
        // head). An fs/read-driven write (`full_extent == false`) sees a
        // warm-sliced PARTIAL extent on a re-run; diffing it against the
        // prior run would wrongly retract unchanged-file rows.
        let graph = ctx.runtime::<RuntimeGraph>();
        if support_input_count == 0 && self.full_extent {
            let epoch = graph.as_ref().map(|g| g.run_epoch()).unwrap_or(0);
            mounted_query::reconcile_owner_table(
                self.store.as_ref(),
                self.table.as_ref(),
                &self.owner_key(),
                epoch,
                &owner_reconcile_ids,
            );
        }
        arced.into_iter().map(Node::Emit).collect()
    }
}

/// Join semantics on FactRead. Default is `Inner` (semi-join: cursor
/// flows N times for N matches, dropped on empty). `Anti` drops on
/// match and passes through on empty — `WHERE NOT EXISTS`.
///
/// TODO: `LeftOuter` — pass cursor with NULL projections on empty,
/// emit each on match. Needed once the language surface lands rules
/// with optional projections.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    Anti,
}

/// `fact?(:name, KEY, [PROJ...])`. Row-SELECT (semi-join by default).
/// Use `FactRead::anti(...)` for `fact?` antijoin / `WHERE NOT EXISTS`.
pub struct FactRead {
    pub store: Arc<dyn FactStore<Cursor>>,
    pub table: Arc<str>,
    pub key_term: Arc<str>,
    pub project: Vec<Arc<str>>,
    pub kind: JoinKind,
}

impl FactRead {
    pub fn new(
        store: Arc<dyn FactStore<Cursor>>,
        table: impl Into<Arc<str>>,
        key_term: impl Into<Arc<str>>,
        project: &[&str],
    ) -> Self {
        Self {
            store,
            table: table.into(),
            key_term: key_term.into(),
            project: project.iter().map(|s| Arc::<str>::from(*s)).collect(),
            kind: JoinKind::Inner,
        }
    }

    /// Antijoin: pass cursor through if NO matches; drop if any.
    /// `fact?(:r, ${A}, ${B})` with all bound → exists-check; this is
    /// the negation. Projection list is ignored (no rows joined in).
    pub fn anti(
        store: Arc<dyn FactStore<Cursor>>,
        table: impl Into<Arc<str>>,
        key_term: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            store,
            table: table.into(),
            key_term: key_term.into(),
            project: Vec::new(),
            kind: JoinKind::Anti,
        }
    }
}

impl Component for FactRead {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(k) = c.get(&self.key_term) else {
            return Node::Done;
        };
        let matches = self.store.read_where(&self.table, &self.key_term, k);

        match self.kind {
            JoinKind::Anti => {
                if matches.is_empty() {
                    Node::Emit(Arc::new(c.clone()))
                } else {
                    Node::Done
                }
            }
            JoinKind::Inner => {
                if matches.is_empty() {
                    return Node::Done;
                }
                let children: Vec<Node<Cursor>> = matches
                    .iter()
                    .map(|row| {
                        let mut child = c.clone();
                        for col in &self.project {
                            if let Some(v) = row.get(col) {
                                child.set(col, v);
                            }
                        }
                        Node::Emit(Arc::new(child))
                    })
                    .collect();
                Node::Many(children)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effect_runtime::v2::{expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend};
    use std::sync::Mutex;

    struct Collector {
        sink: Arc<Mutex<Vec<Cursor>>>,
    }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cursor(value: &str, kvs: &[(&str, &str)]) -> Arc<Cursor> {
        let mut c = Cursor::default();
        c.set_value(value);
        for (k, v) in kvs {
            c.set(k, *v);
        }
        Arc::new(c)
    }

    #[test]
    fn fact_write_inserts_and_passes_through() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(FactWrite::new(store.clone(), "hits")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            queue,
            vec![
                cursor("a", &[("FILE", "x.rs")]),
                cursor("b", &[("FILE", "y.rs")]),
            ],
            ExpandOpts::default(),
        );

        assert_eq!(store.len("hits"), 2);
        assert_eq!(sink.lock().unwrap().len(), 2);
    }

    #[test]
    fn fact_read_cross_products_matches_into_input() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        store.insert("hits", cursor("hi", &[("FILE", "a.rs"), ("HIT", "hi")]));
        store.insert("hits", cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]));
        store.insert("hits", cursor("z", &[("FILE", "b.rs"), ("HIT", "z")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(FactRead::new(store, "hits", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            queue,
            vec![cursor("seed", &[("FILE", "a.rs")])],
            ExpandOpts::default(),
        );

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 2);
        let mut hits: Vec<&str> = got.iter().filter_map(|c| c.get("HIT")).collect();
        hits.sort();
        assert_eq!(hits, vec!["bye", "hi"]);
    }

    #[test]
    fn fact_read_anti_passes_when_no_match_drops_when_match() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        store.insert("seen", cursor("a", &[("FILE", "a.rs")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(FactRead::anti(store, "seen", "FILE")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            queue,
            vec![
                cursor("hit", &[("FILE", "a.rs")]),  // matches → DROP
                cursor("miss", &[("FILE", "b.rs")]), // no match → PASS
            ],
            ExpandOpts::default(),
        );

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].get("FILE"), Some("b.rs"));
    }

    #[test]
    fn fact_read_drops_input_with_no_match() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        store.insert("hits", cursor("hi", &[("FILE", "a.rs"), ("HIT", "hi")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(FactRead::new(store, "hits", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe,
            queue,
            vec![cursor("seed", &[("FILE", "no_match.rs")])],
            ExpandOpts::default(),
        );

        assert_eq!(sink.lock().unwrap().len(), 0);
    }
}
