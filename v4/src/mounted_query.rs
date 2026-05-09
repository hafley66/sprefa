//! Durable query mount helpers.
//!
//! First slice: persist the output set from a batch-local SQL relation op
//! through the existing `FactStore`. Replacement/diff/retraction semantics
//! remain separate follow-up work.

use std::sync::Arc;

use effect_runtime::v2::{FactStore, Next, Node};

use crate::cursor_codec;
use crate::Cursor;

pub const OUTPUT_TABLE: &str = "mounted_query_output";

const MOUNT_ID: &str = "mount_id";
const INPUT_KEY: &str = "input_key";
const GENERATION: &str = "generation";
const OUTPUT_HASH: &str = "output_hash";
const CURSOR_BLOB: &str = "cursor_blob";

pub fn record_sql_outputs(
    store: &Arc<dyn FactStore<Cursor>>,
    sql: &str,
    generation: u64,
    batch: &[&Cursor],
    nodes: &[Node<Cursor>],
) -> Vec<Node<Cursor>> {
    let outputs = output_cursors(nodes);
    store.declare(
        OUTPUT_TABLE,
        &[MOUNT_ID, INPUT_KEY, GENERATION, OUTPUT_HASH, CURSOR_BLOB],
    );

    let mount_id = mount_id_for_sql(sql);
    let input_key = input_key_for_batch(batch);
    let existing_hashes = existing_output_hashes(store, &mount_id, &input_key);
    store.delete_matching(
        OUTPUT_TABLE,
        &[(MOUNT_ID, mount_id.as_str()), (INPUT_KEY, input_key.as_str())],
    );

    if outputs.is_empty() {
        return nodes_added_since(nodes, &existing_hashes);
    }

    let generation = generation.to_string();
    let mut rows = Vec::with_capacity(outputs.len());

    for cursor in outputs {
        let encoded = cursor_codec::encode(&cursor);
        let mut row = Cursor::default();
        row.set(MOUNT_ID, mount_id.clone());
        row.set(INPUT_KEY, input_key.clone());
        row.set(GENERATION, generation.clone());
        row.set(OUTPUT_HASH, hex(blake3::hash(&encoded).as_bytes()));
        row.set(CURSOR_BLOB, hex(&encoded));
        rows.push(Arc::new(row));
    }

    store.insert_batch(OUTPUT_TABLE, rows);
    nodes_added_since(nodes, &existing_hashes)
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

fn output_cursors(nodes: &[Node<Cursor>]) -> Vec<Cursor> {
    let mut out = Vec::new();
    for node in nodes {
        collect_node_outputs(node, &mut out);
    }
    out
}

fn existing_output_hashes(
    store: &Arc<dyn FactStore<Cursor>>,
    mount_id: &str,
    input_key: &str,
) -> std::collections::BTreeSet<String> {
    store
        .rows_of(OUTPUT_TABLE)
        .into_iter()
        .filter(|row| {
            row.get(MOUNT_ID) == Some(mount_id)
                && row.get(INPUT_KEY) == Some(input_key)
        })
        .filter_map(|row| row.get(OUTPUT_HASH).map(|s| s.to_string()))
        .collect()
}

fn nodes_added_since(
    nodes: &[Node<Cursor>],
    existing_hashes: &std::collections::BTreeSet<String>,
) -> Vec<Node<Cursor>> {
    nodes
        .iter()
        .map(|node| filter_node_added_since(node, existing_hashes))
        .collect()
}

fn filter_node_added_since(
    node: &Node<Cursor>,
    existing_hashes: &std::collections::BTreeSet<String>,
) -> Node<Cursor> {
    match node {
        Node::Emit(cursor) => {
            let encoded = cursor_codec::encode(cursor);
            let hash = hex(blake3::hash(&encoded).as_bytes());
            if existing_hashes.contains(&hash) {
                Node::Done
            } else {
                Node::Emit(cursor.clone())
            }
        }
        Node::Many(nodes) => {
            let kept: Vec<Node<Cursor>> = nodes
                .iter()
                .map(|node| filter_node_added_since(node, existing_hashes))
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

fn collect_node_outputs(node: &Node<Cursor>, out: &mut Vec<Cursor>) {
    match node {
        Node::Emit(cursor) => out.push((**cursor).clone()),
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
        match added.as_slice() {
            [Node::Emit(cursor)] => assert_eq!(cursor.value.as_ref(), "new"),
            other => panic!("expected only the new output cursor, got {other:?}"),
        }
    }
}
