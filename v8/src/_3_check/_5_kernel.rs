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

pub const COMPARISONS: [&str; 6] = ["int_lt", "int_le", "int_eq", "int_ne", "int_ge", "int_gt"];

/// `0_lowerer.pl:1958-1973`, in declaration order.
pub const KERNEL_RELATIONS: [(&str, i64); 17] = [
    ("node", 1),
    ("module", 1),
    ("product", 1),
    ("sum", 1),
    (":", 4),
    ("edge_snapshot", 4),
    ("nil", 1),
    ("cons", 3),
    ("edge_ref", 3),
    ("intern", 3),
    ("intern_snapshot", 3),
    ("int_lt", 2),
    ("int_le", 2),
    ("int_eq", 2),
    ("int_ne", 2),
    ("int_ge", 2),
    ("int_gt", 2),
];

/// `:617`.
pub fn primitive_name(name: &str) -> bool {
    matches!(name, "int" | "text" | "any" | "type")
}

pub fn is_comparison(name: &str) -> bool {
    COMPARISONS.contains(&name)
}

/// `:629`. The clause with the `integer_comparison/3` guard sits at `:636`,
/// after the seven explicit names and before `def`.
pub fn kernel_relation_keys(name: &str) -> Vec<Vec<i64>> {
    match name {
        ":" | "edge_snapshot" => vec![vec![0, 1], vec![0, 3]],
        "nil" => vec![vec![0]],
        "cons" => vec![vec![0, 1], vec![2]],
        "edge_ref" | "intern" | "intern_snapshot" => vec![vec![0, 1]],
        other if is_comparison(other) => vec![vec![0, 1]],
        "def" | "head" => vec![vec![0]],
        "body" => vec![vec![0, 1]],
        _ => vec![],
    }
}

fn kernel_ref(u: &mut Universe, name: &str) -> TermId {
    let atom = u.atom(name);
    let kernel = u.compound("kernel", vec![atom]);
    u.compound("ref", vec![kernel])
}

fn primitive_ref(u: &mut Universe, name: &str) -> TermId {
    let atom = u.atom(name);
    let primitive = u.compound("primitive", vec![atom]);
    u.compound("ref", vec![primitive])
}

/// `:622`. Every kernel relation as `relation(ref(kernel(Name)), Arity, Keys)`.
pub fn kernel_relation_rows(u: &mut Universe) -> Vec<TermId> {
    let mut out = Vec::with_capacity(KERNEL_RELATIONS.len() + 3);
    for (name, arity) in
        KERNEL_RELATIONS
            .iter()
            .copied()
            .chain([("def", 2), ("head", 2), ("body", 4)])
    {
        let reference = kernel_ref(u, name);
        let arity = u.int(arity);
        let keys: Vec<TermId> = kernel_relation_keys(name)
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

fn node_pair(u: &mut Universe, name: &str, out: &mut Vec<TermId>) {
    let reference = u.atom(name);
    let kernel = u.compound("kernel", vec![reference]);
    let node = u.compound("node", vec![kernel]);
    let product = u.compound("product", vec![kernel]);
    out.push(node);
    out.push(product);
}

fn edge(u: &mut Universe, owner: &str, label: &str, target: TermId, index: i64) -> TermId {
    let owner = u.atom(owner);
    let owner = u.compound("kernel", vec![owner]);
    let label = u.atom(label);
    let index = u.int(index);
    u.compound(":", vec![owner, label, target, index])
}

/// `:647`. The node order is observable; the edge order is not, because
/// `msort/2` runs at `:425`.
pub fn kernel_graph(u: &mut Universe) -> (Vec<TermId>, Vec<TermId>) {
    let mut nodes = Vec::new();
    for name in ["int", "text", "any", "type"] {
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
        node_pair(u, name, &mut nodes);
    }
    for name in COMPARISONS {
        node_pair(u, name, &mut nodes);
    }
    for name in ["def", "head", "body"] {
        node_pair(u, name, &mut nodes);
    }

    let int = primitive_ref(u, "int");
    let text = primitive_ref(u, "text");
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
    for name in COMPARISONS {
        edges.push(edge(u, name, "left", int, 0));
        edges.push(edge(u, name, "right", int, 1));
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
