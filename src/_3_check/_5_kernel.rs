//! The kernel relation rows and the kernel graph the checker appends to every
//! clean program. Port of `1_checker.pl:617-725`.
//!
//! `kernel_relation_keys/2` (`:629`) is NOT the lowerer's `kernel_keys/1`
//! (`_2_lower/_9_kernel.rs:51`): `nil` is `[[0]]` here and `[[]]` there, and
//! `def`, `head`, `body`, `node`, `module`, `product` and `sum` exist only
//! here. `CheckedNodes` is an unsorted append at `:423`, so `COMPARISONS`
//! keeps the `integer_comparison/3` declaration order of
//! `v7/src/1_libtime/0_evaluator.pl:60-65` rather than the lowerer's.

use crate::_6_eval::term::{TermId, Universe};

pub const COMPARISONS: [&str; 6] = ["lt", "le", "eq", "ne", "ge", "gt"];

/// `0_lowerer.pl:1958-1973`, in declaration order: `(owner, label, arity)`.
pub const KERNEL_RELATIONS: [(Option<&str>, &str, i64); 22] = [
    (None, "node", 1),
    (None, "module", 1),
    (None, "product", 1),
    (None, "sum", 1),
    (None, ":", 4),
    (None, "edge_snapshot", 4),
    (None, "nil", 1),
    (None, "cons", 3),
    (None, "edge_ref", 3),
    (None, "intern", 3),
    (None, "intern_snapshot", 3),
    (Some("int"), "add", 3),
    (None, "effect", 2),
    (Some("any"), "lt", 2),
    (Some("int"), "lt", 2),
    (Some("int"), "le", 2),
    (Some("int"), "eq", 2),
    (Some("int"), "ne", 2),
    (Some("int"), "ge", 2),
    (Some("int"), "gt", 2),
    (Some("str"), "cons", 3),
    (Some("str"), "nil", 1),
];

/// The typed ops, each an edge off its primitive node: `(owner, label, slots)`.
const TYPED_OPS: [(&str, &str, &[(&str, &str)]); 10] = [
    ("int", "lt", &[("left", "int"), ("right", "int")]),
    ("int", "le", &[("left", "int"), ("right", "int")]),
    ("int", "eq", &[("left", "int"), ("right", "int")]),
    ("int", "ne", &[("left", "int"), ("right", "int")]),
    ("int", "ge", &[("left", "int"), ("right", "int")]),
    ("int", "gt", &[("left", "int"), ("right", "int")]),
    ("int", "add", &[("left", "int"), ("right", "int"), ("return", "int")]),
    ("any", "lt", &[("left", "any"), ("right", "any")]),
    ("str", "cons", &[("head", "str"), ("tail", "str"), ("return", "str")]),
    ("str", "nil", &[("return", "str")]),
];

/// `:617`.
pub fn primitive_name(name: &str) -> bool {
    matches!(name, "int" | "float" | "bool" | "str" | "any" | "type")
}

/// `int.lt` and its five siblings.
pub fn is_comparison(owner: Option<&str>, label: &str) -> bool {
    owner == Some("int") && COMPARISONS.contains(&label)
}

/// `:629`. The clause with the `integer_comparison/3` guard sits at `:636`,
/// after the seven explicit names and before `def`.
pub fn kernel_relation_keys(owner: Option<&str>, label: &str) -> Vec<Vec<i64>> {
    match (owner, label) {
        (None, ":" | "edge_snapshot") => vec![vec![0, 1], vec![0, 3]],
        (None | Some("str"), "nil") => vec![vec![0]],
        (None | Some("str"), "cons") => vec![vec![0, 1], vec![2]],
        (None, "edge_ref" | "intern" | "intern_snapshot") => vec![vec![0, 1]],
        (Some("int"), "add") | (None, "effect") | (Some("any"), "lt") => vec![vec![0, 1]],
        _ if is_comparison(owner, label) => vec![vec![0, 1]],
        (None, "def" | "head") => vec![vec![0]],
        (None, "body") => vec![vec![0, 1]],
        _ => vec![],
    }
}

/// `kernel(Label)` or `kernel(Owner, Label)`.
fn kernel_term(u: &mut Universe, owner: Option<&str>, label: &str) -> TermId {
    let label = u.atom(label);
    match owner {
        Some(owner) => {
            let owner = u.atom(owner);
            u.compound("kernel", vec![owner, label])
        }
        None => u.compound("kernel", vec![label]),
    }
}

fn kernel_ref(u: &mut Universe, owner: Option<&str>, label: &str) -> TermId {
    let kernel = kernel_term(u, owner, label);
    u.compound("ref", vec![kernel])
}

fn primitive_ref(u: &mut Universe, name: &str) -> TermId {
    let atom = u.atom(name);
    let primitive = u.compound("primitive", vec![atom]);
    u.compound("ref", vec![primitive])
}

/// `:622`. Every kernel relation as `relation(ref(kernel(..)), Arity, Keys)`.
pub fn kernel_relation_rows(u: &mut Universe) -> Vec<TermId> {
    let mut out = Vec::with_capacity(KERNEL_RELATIONS.len() + 3);
    for (owner, label, arity) in KERNEL_RELATIONS.iter().copied().chain([
        (None, "def", 2),
        (None, "head", 2),
        (None, "body", 4),
    ]) {
        let reference = kernel_ref(u, owner, label);
        let arity = u.int(arity);
        let keys: Vec<TermId> = kernel_relation_keys(owner, label)
            .into_iter()
            .map(|set| {
                let items: Vec<TermId> = set.into_iter().map(|i| u.int(i)).collect();
                u.list(&items)
            })
            .collect();
        let keys = u.list(&keys);
        out.push(u.compound("relation", vec![reference, arity, keys]));
    }
    out
}

fn node_pair(u: &mut Universe, owner: Option<&str>, label: &str, out: &mut Vec<TermId>) {
    let kernel = kernel_term(u, owner, label);
    let node = u.compound("node", vec![kernel]);
    let product = u.compound("product", vec![kernel]);
    out.push(node);
    out.push(product);
}

fn edge(u: &mut Universe, owner: &str, label: &str, target: TermId, index: i64) -> TermId {
    let owner = kernel_term(u, None, owner);
    labelled_edge(u, owner, label, target, index)
}

fn labelled_edge(u: &mut Universe, owner: TermId, label: &str, target: TermId, index: i64) -> TermId {
    let label = u.atom(label);
    let index = u.int(index);
    u.compound(":", vec![owner, label, target, index])
}

/// `:647`. The node order is observable; the edge order is not, because
/// `msort/2` runs at `:425`.
pub fn kernel_graph(u: &mut Universe) -> (Vec<TermId>, Vec<TermId>) {
    let mut nodes = Vec::new();
    for name in ["int", "str", "any", "type"] {
        let atom = u.atom(name);
        let primitive = u.compound("primitive", vec![atom]);
        nodes.push(u.compound("node", vec![primitive]));
    }
    for name in [
        "node",
        "module",
        "product",
        "sum",
        ":",
        "edge_snapshot",
        "nil",
        "cons",
        "edge_ref",
        "intern",
        "intern_snapshot",
    ] {
        node_pair(u, None, name, &mut nodes);
    }
    for (owner, label, _) in TYPED_OPS {
        node_pair(u, Some(owner), label, &mut nodes);
    }
    for name in ["def", "head", "body"] {
        node_pair(u, None, name, &mut nodes);
    }

    let int = primitive_ref(u, "int");
    let text = primitive_ref(u, "str");
    let any = primitive_ref(u, "any");
    let kind = primitive_ref(u, "type");
    let mut edges = Vec::new();
    for name in ["node", "module", "product", "sum"] {
        edges.push(edge(u, name, "id", kind, 0));
    }
    for name in [":", "edge_snapshot"] {
        edges.push(edge(u, name, "owner", kind, 0));
        edges.push(edge(u, name, "name", any, 1));
        edges.push(edge(u, name, "target", any, 2));
        edges.push(edge(u, name, "index", int, 3));
    }
    edges.push(edge(u, "nil", "return", any, 0));
    edges.push(edge(u, "cons", "head", any, 0));
    edges.push(edge(u, "cons", "tail", any, 1));
    edges.push(edge(u, "cons", "return", any, 2));
    edges.push(edge(u, "edge_ref", "owner", kind, 0));
    edges.push(edge(u, "edge_ref", "label", any, 1));
    edges.push(edge(u, "edge_ref", "return", kind, 2));
    for name in ["intern", "intern_snapshot"] {
        edges.push(edge(u, name, "constructor", kind, 0));
        edges.push(edge(u, name, "arguments", any, 1));
        edges.push(edge(u, name, "return", kind, 2));
    }
    let mut ordinals: Vec<(&str, i64)> = Vec::new();
    for (owner, label, slots) in TYPED_OPS {
        let kernel = kernel_term(u, Some(owner), label);
        for (index, (slot, primitive)) in slots.iter().enumerate() {
            let target = primitive_ref(u, primitive);
            edges.push(labelled_edge(u, kernel, slot, target, index as i64));
        }
        let ordinal = match ordinals.iter_mut().find(|(seen, _)| *seen == owner) {
            Some((_, next)) => {
                *next += 1;
                *next
            }
            None => {
                ordinals.push((owner, 0));
                0
            }
        };
        let atom = u.atom(owner);
        let primitive = u.compound("primitive", vec![atom]);
        let reference = u.compound("ref", vec![kernel]);
        edges.push(labelled_edge(u, primitive, label, reference, ordinal));
    }
    edges.push(edge(u, "def", "relation", kind, 0));
    edges.push(edge(u, "def", "arity", int, 1));
    edges.push(edge(u, "head", "rule", kind, 0));
    edges.push(edge(u, "head", "application", kind, 1));
    edges.push(edge(u, "body", "rule", kind, 0));
    edges.push(edge(u, "body", "goal", int, 1));
    edges.push(edge(u, "body", "polarity", text, 2));
    edges.push(edge(u, "body", "application", kind, 3));
    (nodes, edges)
}
