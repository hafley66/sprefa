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
) {
    let outputs = output_cursors(nodes);
    if outputs.is_empty() {
        return;
    }

    store.declare(
        OUTPUT_TABLE,
        &[MOUNT_ID, INPUT_KEY, GENERATION, OUTPUT_HASH, CURSOR_BLOB],
    );

    let mount_id = mount_id_for_sql(sql);
    let input_key = input_key_for_batch(batch);
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
