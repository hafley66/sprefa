//! Durable query mount helpers.
//!
//! First slice: persist the output set from a batch-local SQL relation op
//! through the existing `FactStore`.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, Next, Node};

use crate::cursor_codec;
use crate::Cursor;

pub const OUTPUT_TABLE: &str = "mounted_query_output";
pub const CURSOR_TABLE: &str = "mounted_query_cursor";

const MOUNT_ID: &str = "mount_id";
const INPUT_KEY: &str = "input_key";
const GENERATION: &str = "generation";
const CURSOR_ID: &str = "cursor_id";
const CURSOR_BLOB: &str = "cursor_blob";

pub trait MountedQueryStorage {
    fn record_sql_outputs(
        &self,
        sql: &str,
        generation: u64,
        batch: &[&Cursor],
        nodes: &[Node<Cursor>],
    ) -> Vec<Node<Cursor>>;
}

pub struct FactMountedQueryStorage {
    store: Arc<dyn FactStore<Cursor>>,
}

struct PersistedCursor {
    cursor_id:  String,
    blob_hex:   String,
}

impl FactMountedQueryStorage {
    pub fn new(store: Arc<dyn FactStore<Cursor>>) -> Self {
        Self { store }
    }

    fn declare_tables(&self) {
        self.store.declare(CURSOR_TABLE, &[CURSOR_ID, CURSOR_BLOB]);
        self.store.declare(OUTPUT_TABLE, &[MOUNT_ID, INPUT_KEY, GENERATION, CURSOR_ID]);
    }

    fn existing_cursor_ids(
        &self,
        mount_id: &str,
        input_key: &str,
    ) -> std::collections::HashSet<String> {
        self.store
            .rows_of(OUTPUT_TABLE)
            .into_iter()
            .filter(|row| {
                row.get(MOUNT_ID) == Some(mount_id)
                    && row.get(INPUT_KEY) == Some(input_key)
            })
            .filter_map(|row| row.get(CURSOR_ID).map(|s| s.to_string()))
            .collect()
    }

    fn intern_output_cursors(&self, outputs: &[PersistedCursor]) {
        let rows: Vec<Arc<Cursor>> = outputs
            .iter()
            .map(|persisted| {
                let mut row = Cursor::default();
                row.set(CURSOR_ID, persisted.cursor_id.clone());
                row.set(CURSOR_BLOB, persisted.blob_hex.clone());
                Arc::new(row)
            })
            .collect();
        self.store.insert_batch(CURSOR_TABLE, rows);
    }
}

impl MountedQueryStorage for FactMountedQueryStorage {
    fn record_sql_outputs(
        &self,
        sql: &str,
        generation: u64,
        batch: &[&Cursor],
        nodes: &[Node<Cursor>],
    ) -> Vec<Node<Cursor>> {
        let outputs = output_cursors(nodes);
        let persisted = persist_output_cursors(outputs);
        self.declare_tables();

        let mount_id = mount_id_for_sql(sql);
        let input_key = input_key_for_batch(batch);
        let existing_cursor_ids = self.existing_cursor_ids(&mount_id, &input_key);
        self.store.delete_matching(
            OUTPUT_TABLE,
            &[(MOUNT_ID, mount_id.as_str()), (INPUT_KEY, input_key.as_str())],
        );

        if persisted.is_empty() {
            return nodes_added_since(nodes, &existing_cursor_ids);
        }

        self.intern_output_cursors(&persisted);

        let generation = generation.to_string();
        let mut rows = Vec::with_capacity(persisted.len());

        for persisted in &persisted {
            let mut row = Cursor::default();
            row.set(MOUNT_ID, mount_id.clone());
            row.set(INPUT_KEY, input_key.clone());
            row.set(GENERATION, generation.clone());
            row.set(CURSOR_ID, persisted.cursor_id.clone());
            rows.push(Arc::new(row));
        }

        self.store.insert_batch(OUTPUT_TABLE, rows);
        nodes_added_since(nodes, &existing_cursor_ids)
    }
}

pub fn record_sql_outputs(
    store: &Arc<dyn FactStore<Cursor>>,
    sql: &str,
    generation: u64,
    batch: &[&Cursor],
    nodes: &[Node<Cursor>],
) -> Vec<Node<Cursor>> {
    FactMountedQueryStorage::new(store.clone())
        .record_sql_outputs(sql, generation, batch, nodes)
}

pub fn mount_id_for_sql(sql: &str) -> String {
    hex(blake3::hash(sql.as_bytes()).as_bytes())
}

pub fn input_key_for_batch(batch: &[&Cursor]) -> String {
    let mut h = blake3::Hasher::new();
    for cursor in batch {
        h.update(&cursor.content_hash());
        h.update(b"\0");
    }
    hex(h.finalize().as_bytes())
}

fn output_cursors(nodes: &[Node<Cursor>]) -> Vec<Arc<Cursor>> {
    let mut out = Vec::new();
    for node in nodes {
        collect_node_outputs(node, &mut out);
    }
    out
}

fn nodes_added_since(
    nodes: &[Node<Cursor>],
    existing_cursor_ids: &std::collections::HashSet<String>,
) -> Vec<Node<Cursor>> {
    nodes
        .iter()
        .map(|node| filter_node_added_since(node, existing_cursor_ids))
        .collect()
}

fn filter_node_added_since(
    node: &Node<Cursor>,
    existing_cursor_ids: &std::collections::HashSet<String>,
) -> Node<Cursor> {
    match node {
        Node::Emit(cursor) => {
            let (cursor_id, _) = cursor_storage_parts(cursor);
            if existing_cursor_ids.contains(&cursor_id) {
                Node::Done
            } else {
                Node::Emit(cursor.clone())
            }
        }
        Node::Many(nodes) => {
            let kept: Vec<Node<Cursor>> = nodes
                .iter()
                .map(|node| filter_node_added_since(node, existing_cursor_ids))
                .filter(|node| !matches!(node, Node::Done))
                .collect();
            match kept.len() {
                0 => Node::Done,
                1 => kept.into_iter().next().unwrap(),
                _ => Node::Many(kept),
            }
        }
        Node::Yield { value, wake } => Node::Yield {
            value: value.clone(),
            wake: wake.clone(),
        },
        Node::Done => Node::Done,
    }
}

fn cursor_storage_parts(cursor: &Cursor) -> (String, Vec<u8>) {
    let encoded = cursor_codec::encode(cursor);
    let cursor_id = hex(blake3::hash(&encoded).as_bytes());
    (cursor_id, encoded)
}

fn persist_output_cursors(outputs: Vec<Arc<Cursor>>) -> Vec<PersistedCursor> {
    outputs
        .into_iter()
        .map(|cursor| {
            let (cursor_id, encoded) = cursor_storage_parts(&cursor);
            PersistedCursor {
                cursor_id,
                blob_hex: hex(&encoded),
            }
        })
        .collect()
}

fn collect_node_outputs(node: &Node<Cursor>, out: &mut Vec<Arc<Cursor>>) {
    match node {
        Node::Emit(cursor) => out.push(cursor.clone()),
        Node::Many(nodes) => {
            for child in nodes {
                collect_node_outputs(child, out);
            }
        }
        Node::Yield { .. } => {}
        Node::Done => {}
    }
}

fn hex(bytes: &[u8]) -> String {
    const CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(CHARS[(byte >> 4) as usize] as char);
        out.push(CHARS[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use effect_runtime::v2::{FactStore, MemFactStore, Node};

    use super::*;

    fn cursor(value: &str) -> Cursor {
        let mut c = Cursor::default();
        c.value = Arc::from(value);
        c
    }

    #[test]
    fn record_sql_outputs_replaces_same_mount_and_input_key() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let first = vec![Node::Emit(Arc::new(cursor("old")))];
        let second = vec![Node::Emit(Arc::new(cursor("new")))];

        let added = record_sql_outputs(&store, "SELECT value FROM input", 1, &batch, &first);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 1);

        let added = record_sql_outputs(&store, "SELECT value FROM input", 2, &batch, &second);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        let rows = store.rows_of(OUTPUT_TABLE);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get(GENERATION), Some("2"));
        assert!(rows[0].get(CURSOR_ID).is_some());
        assert_eq!(store.rows_of(CURSOR_TABLE).len(), 2);
    }

    #[test]
    fn record_sql_outputs_removes_same_mount_when_new_result_is_empty() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let first = vec![Node::Emit(Arc::new(cursor("old")))];
        let empty = vec![Node::Done];

        let added = record_sql_outputs(&store, "SELECT value FROM input", 1, &batch, &first);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 1);

        let added = record_sql_outputs(&store, "SELECT value FROM input", 2, &batch, &empty);
        assert!(matches!(added.as_slice(), [Node::Done]));
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 0);
        assert_eq!(store.rows_of(CURSOR_TABLE).len(), 1);
    }

    #[test]
    fn record_sql_outputs_returns_only_new_outputs_for_same_mount() {
        let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let input = cursor("seed");
        let batch = vec![&input];
        let old = Arc::new(cursor("old"));
        let new = Arc::new(cursor("new"));

        let first = vec![Node::Emit(old.clone())];
        let added = record_sql_outputs(&store, "SELECT value FROM input", 1, &batch, &first);
        assert!(matches!(added.as_slice(), [Node::Emit(_)]));

        let rerun = vec![Node::Many(vec![
            Node::Emit(old),
            Node::Emit(new.clone()),
        ])];
        let added = record_sql_outputs(&store, "SELECT value FROM input", 2, &batch, &rerun);
        assert_eq!(store.rows_of(OUTPUT_TABLE).len(), 2);
        assert_eq!(store.rows_of(CURSOR_TABLE).len(), 2);
        match added.as_slice() {
            [Node::Emit(cursor)] => assert_eq!(cursor.value.as_ref(), "new"),
            other => panic!("expected only the new output cursor, got {other:?}"),
        }
    }
}
