//! `Fact` — wide-row table store + Components.
//!
//!   `fact(:name) > FactWrite { cols }`   row-INSERT
//!   `fact?(:name, KEY, [PROJ...])`       row-SELECT WHERE KEY = cursor[KEY]
//!
//! In-memory FactStore for now. Each table is a `Vec<Arc<Cursor>>`;
//! reads scan linearly. Persistence and indexing arrive when this
//! lifts onto a sqlite-backed FactStore (deferred). The Component
//! surface stays the same when persistence lands.
//!
//! FactStore is built once per app and passed by `Arc<FactStore>` into
//! the FactWrite/FactRead constructors. No globals.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::{Component, Node, RenderCtx};

use crate::Cursor;

/// In-memory wide-row store. `tables: name → rows`. Append on insert;
/// linear scan on read. Concurrency: one mutex over the whole map.
#[derive(Default)]
pub struct FactStore {
    tables: Mutex<HashMap<String, Vec<Arc<Cursor>>>>,
}

impl FactStore {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&self, table: &str, row: Arc<Cursor>) {
        self.tables
            .lock()
            .unwrap()
            .entry(table.to_string())
            .or_default()
            .push(row);
    }

    /// Return all rows where `row.get(col) == value`.
    pub fn read_where(&self, table: &str, col: &str, value: &str) -> Vec<Arc<Cursor>> {
        let g = self.tables.lock().unwrap();
        let Some(rows) = g.get(table) else { return Vec::new() };
        rows.iter()
            .filter(|r| r.get(col).map(|v| v == value).unwrap_or(false))
            .cloned()
            .collect()
    }

    pub fn rows_of(&self, table: &str) -> Vec<Arc<Cursor>> {
        self.tables
            .lock()
            .unwrap()
            .get(table)
            .cloned()
            .unwrap_or_default()
    }

    pub fn len(&self, table: &str) -> usize {
        self.tables.lock().unwrap().get(table).map(|v| v.len()).unwrap_or(0)
    }
}

/// `fact(:name) > FactWrite { cols }`. Row-INSERT. Pass-through.
pub struct FactWrite {
    pub store: Arc<FactStore>,
    pub table: Arc<str>,
}

impl FactWrite {
    pub fn new(store: Arc<FactStore>, table: impl Into<Arc<str>>) -> Self {
        Self { store, table: table.into() }
    }
}

impl Component for FactWrite {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.store.insert(&self.table, Arc::new(c.clone()));
        Node::Emit(Arc::new(c.clone()))
    }
}

/// `fact?(:name, KEY, [PROJ...])`. Row-SELECT.
///
/// Cross-product: each input cursor matches against rows where the
/// stored row's `KEY` column equals input cursor's KEY. Each match
/// emits one child cursor with `project` columns copied from the row
/// onto a clone of the input. Input cursors with no KEY or no matches
/// drop.
pub struct FactRead {
    pub store:    Arc<FactStore>,
    pub table:    Arc<str>,
    pub key_term: Arc<str>,
    pub project:  Vec<Arc<str>>,
}

impl FactRead {
    pub fn new(
        store: Arc<FactStore>,
        table: impl Into<Arc<str>>,
        key_term: impl Into<Arc<str>>,
        project: &[&str],
    ) -> Self {
        Self {
            store,
            table:    table.into(),
            key_term: key_term.into(),
            project:  project.iter().map(|s| Arc::<str>::from(*s)).collect(),
        }
    }
}

impl Component for FactRead {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let Some(k) = c.get(&self.key_term) else { return Node::Done };
        let matches = self.store.read_where(&self.table, &self.key_term, k);
        if matches.is_empty() { return Node::Done; }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use effect_runtime::v2::{
        expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend,
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
    fn fact_store_insert_and_read_where() {
        let s = FactStore::new();
        s.insert("strings", cursor("hi",  &[("FILE", "a.rs"), ("HIT", "hi")]));
        s.insert("strings", cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]));
        s.insert("strings", cursor("hi",  &[("FILE", "b.rs"), ("HIT", "hi")]));

        let by_a = s.read_where("strings", "FILE", "a.rs");
        assert_eq!(by_a.len(), 2);
        assert_eq!(s.len("strings"), 3);
    }

    #[test]
    fn fact_write_inserts_and_passes_through() {
        let store = Arc::new(FactStore::new());
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(FactWrite::new(store.clone(), "hits")) as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe, queue,
            vec![
                cursor("a", &[("FILE", "x.rs")]),
                cursor("b", &[("FILE", "y.rs")]),
            ],
            ExpandOpts::default(),
        );

        assert_eq!(store.len("hits"), 2);
        assert_eq!(sink.lock().unwrap().len(), 2);   // pass-through
    }

    #[test]
    fn fact_read_cross_products_matches_into_input() {
        let store = Arc::new(FactStore::new());
        store.insert("hits", cursor("hi",  &[("FILE", "a.rs"), ("HIT", "hi")]));
        store.insert("hits", cursor("bye", &[("FILE", "a.rs"), ("HIT", "bye")]));
        store.insert("hits", cursor("z",   &[("FILE", "b.rs"), ("HIT", "z")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(FactRead::new(store, "hits", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe, queue,
            vec![cursor("seed", &[("FILE", "a.rs")])],
            ExpandOpts::default(),
        );

        let got = sink.lock().unwrap();
        assert_eq!(got.len(), 2);                          // 2 matches in a.rs
        let mut hits: Vec<&str> = got.iter()
            .filter_map(|c| c.get("HIT")).collect();
        hits.sort();
        assert_eq!(hits, vec!["bye", "hi"]);
    }

    #[test]
    fn fact_read_drops_input_with_no_match() {
        let store = Arc::new(FactStore::new());
        store.insert("hits", cursor("hi", &[("FILE", "a.rs"), ("HIT", "hi")]));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let sink  = Arc::new(Mutex::new(Vec::new()));
        let pipe  = PipeInstance::new(vec![
            Arc::new(FactRead::new(store, "hits", "FILE", &["HIT"]))
                as Arc<dyn Component<Next = Cursor>>,
            Arc::new(Collector { sink: sink.clone() }),
        ]);

        expand(
            &pipe, queue,
            vec![cursor("seed", &[("FILE", "no_match.rs")])],
            ExpandOpts::default(),
        );

        assert_eq!(sink.lock().unwrap().len(), 0);
    }
}
