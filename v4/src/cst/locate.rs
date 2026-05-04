//! locate(byte) → Located, resolve(path) → Node.
//!
//! Descent rule: an `Injected` covers byte range `host_off..host_off+source.len()`.
//! `locate` walks down through every `Injected` whose range contains the byte,
//! shifting the offset. The deepest tree wins.

use std::sync::Arc;

use crate::cst::doc::{Doc, DocId};
use crate::cst::injected::Injected;
use crate::cst::path::{items, Path, PathBuilder, PathItem};

pub struct Located<'a> {
    pub doc_id:    DocId,
    pub path:      Path,
    pub host_off:  usize,
    pub dsl_id:    Arc<str>,
    pub node:      tree_sitter::Node<'a>,
    pub trail:     Vec<Path>,
}

pub fn locate<'a>(doc: &'a Doc, host_byte: usize) -> Located<'a> {
    // Start from the host tree.
    let host_root = doc.tree.root_node();
    let mut current_node = node_for_byte(host_root, host_byte);
    let mut current_path = ts_path_to(host_root, current_node);
    let mut current_off: usize = 0;
    let mut current_dsl: Arc<str> = Arc::from(""); // host DSL has no id at this layer
    let mut trail: Vec<Path> = Vec::new();

    let mut active_children: &[Injected] = &doc.injected;
    let mut active_source: &str = &doc.source;
    let _ = active_source;

    'descend: loop {
        for inj in active_children {
            let body_len = inj.source.len();
            let span = inj.host_off..inj.host_off + body_len;
            // Convert outer host_byte to a position relative to this inj's source.
            // host_byte is in the OUTER coord system; inj.host_off is also in the outer coord.
            if host_byte >= span.start && host_byte < span.end {
                // descend
                trail.push(inj.host_path.clone());
                let local_byte = host_byte - inj.host_off;
                let inj_root = inj.tree.root_node();
                let inj_node = node_for_byte(inj_root, local_byte);
                // build full path: host_path of inj + Body + ts_path inside the sub-tree
                let mut b = PathBuilder::new();
                b.extend_from(&inj.host_path);
                b.push(items::Body);
                let inner = ts_path_to(inj_root, inj_node);
                for it in inner.iter() {
                    b.push_boxed(clone_box(it.as_ref()));
                }
                current_path = b.build();
                current_node = inj_node;
                current_off = inj.host_off;
                current_dsl = inj.dsl_id.clone();
                active_children = &inj.injected;
                active_source = &inj.source;
                let _ = active_source;
                continue 'descend;
            }
        }
        break;
    }

    Located {
        doc_id: doc.doc_id.clone(),
        path: current_path,
        host_off: current_off,
        dsl_id: current_dsl,
        node: current_node,
        trail,
    }
}

/// Walk the tree downward picking the deepest named node whose range contains
/// the byte. Falls back to the root if no descendant matches.
fn node_for_byte<'a>(root: tree_sitter::Node<'a>, byte: usize) -> tree_sitter::Node<'a> {
    root.descendant_for_byte_range(byte, byte).unwrap_or(root)
}

/// Compute a CST path (sequence of `Field { kind_id, idx }`) from `root` down
/// to `target`. Walk up from target collecting parent indices, then reverse.
fn ts_path_to(root: tree_sitter::Node<'_>, target: tree_sitter::Node<'_>) -> Path {
    if target.id() == root.id() {
        return PathBuilder::new().build();
    }
    let mut chain: Vec<items::Field> = Vec::new();
    let mut cursor = target;
    while let Some(parent) = cursor.parent() {
        let kind_id = cursor.kind_id();
        let mut idx: u16 = 0;
        let mut walker = parent.walk();
        let mut hit = false;
        for (i, child) in parent.children(&mut walker).enumerate() {
            if child.id() == cursor.id() {
                idx = u16::try_from(i).unwrap_or(u16::MAX);
                hit = true;
                break;
            }
        }
        let _ = hit;
        chain.push(items::Field { kind_id, idx });
        if parent.id() == root.id() { break; }
        cursor = parent;
    }
    chain.reverse();
    let mut b = PathBuilder::with_capacity(chain.len());
    for f in chain { b.push(f); }
    b.build()
}

/// Resolve a path back to a node within `doc`. Returns None if any step is
/// out of bounds.
///
/// Path shape: a full path is a sequence of segment runs separated by `Body`
/// items. The first run is the host slot address (whatever the host emits;
/// matched literally against `Injected.host_path`). After a `Body`, the next
/// run navigates within that injected sub-tree by `Field` items. This
/// repeats for nested injections.
pub fn resolve<'a>(doc: &'a Doc, path: &Path) -> Option<tree_sitter::Node<'a>> {
    let mut active_tree: &tree_sitter::Tree = &doc.tree;
    let mut active_children: &[Injected] = &doc.injected;
    let mut i = 0usize;
    let mut last_node: Option<tree_sitter::Node<'a>> = None;

    while i < path.len() {
        // Find the next Body or end of path.
        let body_at = (i..path.len()).find(|&j| path[j].kind_tag() == "Body");
        let run_end = body_at.unwrap_or(path.len());

        if let Some(body_idx) = body_at {
            // i..body_idx is the host slot address of an Injected child.
            // Match against active_children by host_path.
            let mut b = PathBuilder::with_capacity(body_idx - i);
            for it in &path[i..body_idx] {
                b.push_boxed(clone_box(it.as_ref()));
            }
            let host_path = b.build();
            let inj = active_children.iter().find(|inj| paths_eq(&inj.host_path, &host_path))?;
            active_tree = &inj.tree;
            active_children = &inj.injected;
            last_node = Some(active_tree.root_node());
            i = body_idx + 1;
            continue;
        }

        // i..run_end == i..path.len() — final TS-field navigation in active_tree.
        let mut node = active_tree.root_node();
        for it in &path[i..run_end] {
            let f = it.as_any().downcast_ref::<items::Field>()?;
            let mut walker = node.walk();
            let children: Vec<_> = node.children(&mut walker).collect();
            let child = children.get(f.idx as usize)?;
            if child.kind_id() != f.kind_id { return None; }
            node = *child;
        }
        last_node = Some(node);
        i = run_end;
    }

    // No Body, no field steps → either the path was empty, or a host slot
    // address matched but produced no descent.
    if last_node.is_none() {
        last_node = Some(doc.tree.root_node());
    }
    last_node
}

fn paths_eq(a: &Path, b: &Path) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).all(|(x, y)| x.dyn_eq(y.as_ref()))
}

fn clone_box(it: &dyn PathItem) -> Box<dyn PathItem> {
    match it.kind_tag() {
        "Index" => Box::new(it.as_any().downcast_ref::<items::Index>().unwrap().clone()),
        "Name"  => Box::new(it.as_any().downcast_ref::<items::Name>().unwrap().clone()),
        "Field" => Box::new(it.as_any().downcast_ref::<items::Field>().unwrap().clone()),
        "Body"  => Box::new(items::Body),
        other   => panic!("locate: unhandled PathItem kind {other:?}"),
    }
}
