//! `Rule` — a callable, parametric pipeline whose output sinks to a fact.
//!
//! Sprefa's `rule` is a named `Pipe<Cursor>` paired with a sink fact
//! table. Calling the rule = building `body > FactWrite(sink)` and
//! expanding it over a caller-supplied seed.
//!
//! `rule?(:name, KEY, [PROJ])` reads the rule's sink table — a
//! `FactRead` aimed at the same store/table. Constructor sugar lives
//! here as `Rule::query(...)`.
//!
//! Layer-2 input-set memoization (the `Op::probe` path) is deferred;
//! arrives once Component grows a `probe()` shim.

use std::sync::Arc;

use effect_runtime::v2::{
    expand, Component, ExpandOpts, ExpandStats, Pipe, QueueBackend,
};

use crate::Cursor;
use crate::fact::{FactRead, FactWrite};
use effect_runtime::v2::FactStore;
#[cfg(test)]
use effect_runtime::v2::MemFactStore;
// Note: FactStore is now a trait. Rule constructors take
// `Arc<dyn FactStore<Cursor>>` so callers can pick MemFactStore or
// SqliteFactStore at the seam.

/// A callable rule. `body` is the pipe that produces rows; `sink_table`
/// is where they land. Cloning is cheap (Arc-shaped fields).
#[derive(Clone)]
pub struct Rule {
    pub name:       Arc<str>,
    pub store:      Arc<dyn FactStore<Cursor>>,
    pub sink_table: Arc<str>,
    pub sink_cols:  Arc<Vec<Arc<str>>>,
    body_steps:     Arc<Vec<Arc<dyn Component<Next = Cursor>>>>,
}

impl Rule {
    pub fn new(
        name:       impl Into<Arc<str>>,
        store:      Arc<dyn FactStore<Cursor>>,
        sink_table: impl Into<Arc<str>>,
        sink_cols:  &[&str],
        body:       Pipe<Cursor>,
    ) -> Self {
        let sink_table: Arc<str> = sink_table.into();
        store.declare(&sink_table, sink_cols);
        let cols: Vec<Arc<str>> = sink_cols.iter().map(|s| Arc::<str>::from(*s)).collect();
        Self {
            name:       name.into(),
            store,
            sink_table,
            sink_cols:  Arc::new(cols),
            body_steps: Arc::new(body.steps),
        }
    }

    /// `rule = fact` shape: empty body, declaration-only. The first
    /// FactWrite still runs its insert side; `run_with` over an empty
    /// seed is a declaration no-op.
    pub fn passthrough(
        name:       impl Into<Arc<str>>,
        store:      Arc<dyn FactStore<Cursor>>,
        sink_table: impl Into<Arc<str>>,
        sink_cols:  &[&str],
    ) -> Self {
        Self::new(name, store, sink_table, sink_cols, Pipe::new())
    }

    pub fn is_passthrough(&self) -> bool { self.body_steps.is_empty() }

    /// Build the executable pipe (`body > FactWrite(sink)`).
    pub fn into_pipe(self) -> Pipe<Cursor> {
        let mut steps: Vec<Arc<dyn Component<Next = Cursor>>> =
            (*self.body_steps).clone();
        steps.push(Arc::new(FactWrite::new(self.store, self.sink_table)));
        Pipe::from_steps(steps)
    }

    /// Run this rule over caller-supplied seed cursors. Builds the
    /// pipe instance and calls `expand`. Bypass for the input-set
    /// memoization path; seed is the caller's, not the rule's.
    pub fn run_with(
        self,
        queue: Arc<dyn QueueBackend<Cursor>>,
        seed:  Vec<Arc<Cursor>>,
        opts:  ExpandOpts,
    ) -> ExpandStats {
        let inst = self.into_pipe().into_instance();
        expand(&inst, queue, seed, opts)
    }

    /// `rule?` — produce a `FactRead` Component aimed at this rule's
    /// sink table. The Component slots into any pipe.
    pub fn query(
        store:      Arc<dyn FactStore<Cursor>>,
        sink_table: impl Into<Arc<str>>,
        key_term:   impl Into<Arc<str>>,
        project:    &[&str],
    ) -> FactRead {
        FactRead::new(store, sink_table, key_term, project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        MemQueue, Node, PipeInstance, QueueBackend, RenderCtx,
    };

    struct Collector { sink: Arc<Mutex<Vec<Cursor>>> }
    impl Component for Collector {
        type Next = Cursor;
        fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            self.sink.lock().unwrap().push(c.clone());
            Node::Done
        }
    }

    fn cursor(value: &str, kvs: &[(&str, &str)]) -> Arc<Cursor> {
        let mut c = Cursor { value: value.into(), terms: Vec::new() };
        for (k, v) in kvs { c.set(k, *v); }
        Arc::new(c)
    }

    #[test]
    fn passthrough_rule_writes_seed_into_sink() {
        let store = Arc::new(MemFactStore::<Cursor>::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let rule = Rule::passthrough("noop", store.clone(), "out", &["FILE"]);

        rule.run_with(
            queue,
            vec![
                cursor("a", &[("FILE", "x.rs")]),
                cursor("b", &[("FILE", "y.rs")]),
            ],
            ExpandOpts::default(),
        );

        assert_eq!(store.len("out"), 2);
    }

    #[test]
    fn rule_with_body_transforms_then_writes() {
        // body: a tiny Component that uppercases cursor.value.
        struct Upcase;
        impl Component for Upcase {
            type Next = Cursor;
            fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
                let mut next = c.clone();
                next.value = Arc::from(c.value.to_uppercase().as_str());
                Node::Emit(Arc::new(next))
            }
        }

        let store = Arc::new(MemFactStore::<Cursor>::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let body  = Pipe::new().step(Arc::new(Upcase));
        let rule  = Rule::new("upcase", store.clone(), "loud", &[], body);

        rule.run_with(queue, vec![cursor("hi", &[])], ExpandOpts::default());

        let rows = store.rows_of("loud");
        assert_eq!(rows.len(), 1);
        assert_eq!(&*rows[0].value, "HI");
    }

    #[test]
    fn rule_query_reads_from_sink_table() {
        // Step 1: populate via run_with.
        let store = Arc::new(MemFactStore::<Cursor>::new());
        let q1: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let rule = Rule::passthrough("strings", store.clone(), "strings", &["FILE", "HIT"]);
        rule.run_with(q1,
            vec![
                cursor("hi",  &[("FILE", "a.rs"), ("HIT", "hi")]),
                cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]),
                cursor("z",   &[("FILE", "b.rs"), ("HIT", "z")]),
            ],
            ExpandOpts::default(),
        );
        assert_eq!(store.len("strings"), 3);

        // Step 2: query via Rule::query. Pipe: [Reader, Collector].
        let q2: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink = Arc::new(Mutex::new(Vec::new()));
        let pipe = PipeInstance::new(vec![
            Arc::new(Rule::query(store.clone(), "strings", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(&pipe, q2, vec![cursor("seed", &[("FILE", "a.rs")])], ExpandOpts::default());

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 2);
    }
}
