//! The go syntax tier's TSI rows under `--witness --family type`: the twin of
//! `rust_type_edges.rs` `tsi_rows`, beside the v5 entity port in `go.rs`.

use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::Node;

use crate::family::TypeF;
use crate::rows::FamilyBundle;
use crate::shape::{Span, Strings};
use crate::tsi::Arg;
use crate::types::TsiNames;

use super::go::{go_node_span, go_text};

/// Type-parameter names in scope, innermost declaration last.
type TsiScope = BTreeMap<String, u32>;

/// Per-file bookkeeping: ids whose application rows are written, the id each
/// primitive class took, and each owner's next method position.
#[derive(Default)]
struct TsiState {
    called: BTreeSet<u32>,
    classes: BTreeMap<&'static str, u32>,
    methods: BTreeMap<u32, i64>,
}

/// The type names go declares in the universe scope as primitives. `error`
/// and `any` are an interface and an alias there, so they are not listed.
const PRIMITIVE_CLASSES: &[&str] = &[
    "bool", "string", "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16",
    "uint32", "uint64", "uintptr", "float32", "float64", "complex64", "complex128", "byte",
    "rune",
];

/// Package-level declarations only: a declaration inside a function body is
/// the checker's row.
pub(crate) fn tsi_rows(
    root: Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let span = crate::trace::phase_span("go", crate::trace::Phase::TsiSyntax);
    let _entered = span.enter();
    let mut names = TsiNames::new("go");
    let outer = TsiScope::new();
    let mut state = TsiState::default();
    predeclare_types(root, src, strings, &mut names);
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        tsi_declaration(child, &outer, src, strings, &mut names, &mut state);
    }
    sink.aux.tsi = names.into_facts();
    crate::trace::record_phase(&span, 0, sink.aux.tsi.len() as u64, 1);
}

/// A declared type's id origins at its own name even when a reference to it
/// is written earlier in the file, so every declared name is minted first.
fn predeclare_types(root: Node, src: &[u8], strings: &mut Strings, names: &mut TsiNames) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "type_declaration" {
            continue;
        }
        let mut specs = child.walk();
        for spec in child.children(&mut specs) {
            if spec.kind() != "type_spec" {
                continue;
            }
            if let Some(name_node) = spec.child_by_field_name("name") {
                let text = go_text(name_node, src);
                if !PRIMITIVE_CLASSES.contains(&text) {
                    names.named(strings, text, go_node_span(name_node));
                }
            }
        }
    }
}

fn tsi_declaration(
    node: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    match node.kind() {
        "type_declaration" => {
            let mut specs = node.walk();
            for spec in node.children(&mut specs) {
                match spec.kind() {
                    "type_spec" => tsi_type_spec(spec, outer, src, strings, names, state),
                    "type_alias" => tsi_type_alias(spec, outer, src, strings, names, state),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn tsi_type_spec(
    spec: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let (Some(name_node), Some(ty)) = (
        spec.child_by_field_name("name"),
        spec.child_by_field_name("type"),
    ) else {
        return;
    };
    let owner = names.named(strings, go_text(name_node, src), go_node_span(name_node));
    let scope = outer.clone();
    match ty.kind() {
        "struct_type" => {
            names.fact("tsi.product", vec![Arg::Id(owner)]);
        }
        "interface_type" => {
            names.fact("tsi.product", vec![Arg::Id(owner)]);
            names.fact("go.interface", vec![Arg::Id(owner)]);
        }
        _ => {
            let target = tsi_type_id(ty, &scope, src, strings, names, state);
            names.edge(owner, "underlying", target, 0);
        }
    }
}

/// An alias mints no type of its own: a symbol that denotes the written type.
fn tsi_type_alias(
    spec: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let (Some(name_node), Some(ty)) = (
        spec.child_by_field_name("name"),
        spec.child_by_field_name("type"),
    ) else {
        return;
    };
    let symbol = names.bare_id();
    names.fact("tsi.symbol", vec![Arg::Id(symbol)]);
    names.name(symbol, go_text(name_node, src));
    let scope = outer.clone();
    let target = tsi_type_id(ty, &scope, src, strings, names, state);
    names.fact("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(target)]);
}

/// One id per written text except a scoped parameter (rule 4) or a primitive;
/// `*T` and `~T` are `T`; slice, map, chan, array and func keep text only.
fn tsi_type_id(
    node: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let node = strip_type(node);
    let text = go_text(node, src);
    if let Some(&id) = scope.get(text) {
        return id;
    }
    let id = names.named(strings, text, origin_span(node));
    id
}

fn strip_type(node: Node) -> Node {
    let mut node = node;
    loop {
        match node.kind() {
            "pointer_type" | "parenthesized_type" | "negated_type" => match node.named_child(0) {
                Some(inner) => node = inner,
                None => return node,
            },
            _ => return node,
        }
    }
}

/// The last segment of a qualified or generic name is the written type; the
/// rest qualifies or applies it.
fn origin_span(node: Node) -> Span {
    match node.kind() {
        "qualified_type" => node
            .child_by_field_name("name")
            .map(go_node_span)
            .unwrap_or_else(|| go_node_span(node)),
        "generic_type" => node
            .child_by_field_name("type")
            .map(origin_span)
            .unwrap_or_else(|| go_node_span(node)),
        _ => go_node_span(node),
    }
}

/// Every child under a repeated field name, in source order.
fn field_children<'tree>(node: Node<'tree>, field: &str) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    let found: Vec<Node<'tree>> = node.children_by_field_name(field, &mut cursor).collect();
    found
}
