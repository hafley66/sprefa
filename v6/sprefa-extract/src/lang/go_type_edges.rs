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
        "function_declaration" => {
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            tsi_callable(node, name_node, outer, src, strings, names, state);
        }
        "method_declaration" => tsi_method(node, outer, src, strings, names, state),
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
    let scope = tsi_generics(
        owner,
        spec.child_by_field_name("type_parameters"),
        outer,
        src,
        strings,
        names,
        state,
    );
    match ty.kind() {
        "struct_type" => {
            names.fact("tsi.product", vec![Arg::Id(owner)]);
            tsi_struct_fields(owner, ty, &scope, src, strings, names, state);
        }
        "interface_type" => {
            names.fact("tsi.product", vec![Arg::Id(owner)]);
            names.fact("go.interface", vec![Arg::Id(owner)]);
            tsi_interface_members(owner, ty, &scope, src, strings, names, state);
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
    let scope = tsi_generics(
        symbol,
        spec.child_by_field_name("type_parameters"),
        outer,
        src,
        strings,
        names,
        state,
    );
    let target = tsi_type_id(ty, &scope, src, strings, names, state);
    names.fact("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(target)]);
}

/// One `tsi.parameter` per declared name (`[K, V any]` declares two sharing
/// one constraint) plus a `bound` edge per constraint term; hands back the scope.
fn tsi_generics(
    owner: u32,
    list: Option<Node>,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> TsiScope {
    let mut scope = outer.clone();
    let Some(list) = list else {
        return scope;
    };
    let mut position = 0i64;
    let mut cursor = list.walk();
    for declared in list
        .children(&mut cursor)
        .filter(|node| node.kind() == "type_parameter_declaration")
    {
        let terms = declared
            .child_by_field_name("type")
            .map(|constraint| constraint_terms(constraint, src))
            .unwrap_or_default();
        for name_node in field_children(declared, "name") {
            let id = names.anonymous(go_node_span(name_node));
            let written = go_text(name_node, src);
            names.name(id, written);
            names.fact(
                "tsi.parameter",
                vec![
                    Arg::Id(id),
                    Arg::Id(owner),
                    Arg::Int(position),
                    Arg::Atom("unspecified".to_string()),
                ],
            );
            for (rank, term) in terms.iter().enumerate() {
                let target = tsi_type_id(*term, &scope, src, strings, names, state);
                names.edge(id, "bound", target, rank as i64);
            }
            scope.insert(written.to_string(), id);
            position += 1;
        }
    }
    scope
}

/// The terms of a constraint, `any` dropped: it bounds nothing.
fn constraint_terms<'tree>(constraint: Node<'tree>, src: &[u8]) -> Vec<Node<'tree>> {
    let mut terms = Vec::new();
    let mut cursor = constraint.walk();
    for child in constraint.named_children(&mut cursor) {
        if child.kind() == "type_elem" {
            let mut inner = child.walk();
            terms.extend(child.named_children(&mut inner));
        } else {
            terms.push(child);
        }
    }
    terms.retain(|term| go_text(*term, src) != "any");
    terms
}

/// A named field is an edge under its name; an embedded field is an edge
/// under the embedded type's own last segment plus `go.embedding`.
fn tsi_struct_fields(
    owner: u32,
    struct_type: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let mut cursor = struct_type.walk();
    let Some(list) = struct_type
        .children(&mut cursor)
        .find(|node| node.kind() == "field_declaration_list")
    else {
        return;
    };
    let mut position = 0i64;
    let mut fields = list.walk();
    for field in list
        .children(&mut fields)
        .filter(|node| node.kind() == "field_declaration")
    {
        let Some(ty) = field.child_by_field_name("type") else {
            continue;
        };
        let target = tsi_type_id(ty, scope, src, strings, names, state);
        let declared = field_children(field, "name");
        if declared.is_empty() {
            let label = last_segment_text(ty, src);
            names.edge(owner, &label, target, position);
            names.fact("go.embedding", vec![Arg::Id(owner), Arg::Id(target)]);
            position += 1;
            continue;
        }
        for name_node in declared {
            names.edge(owner, go_text(name_node, src), target, position);
            position += 1;
        }
    }
}

/// A method element is a callable the interface reaches by name; a single
/// plain type element is an embedding; a union or a `~` term is a type set.
fn tsi_interface_members(
    owner: u32,
    interface_type: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let mut position = 0i64;
    let mut cursor = interface_type.walk();
    for member in interface_type.named_children(&mut cursor) {
        match member.kind() {
            "method_elem" => {
                let Some(name_node) = member.child_by_field_name("name") else {
                    continue;
                };
                let callable = tsi_callable(member, name_node, scope, src, strings, names, state);
                names.edge(owner, go_text(name_node, src), callable, position);
                position += 1;
            }
            "type_elem" => {
                let mut inner = member.walk();
                let terms: Vec<Node> = member.named_children(&mut inner).collect();
                let embedded = terms.len() == 1 && terms[0].kind() != "negated_type";
                for term in terms {
                    let target = tsi_type_id(term, scope, src, strings, names, state);
                    let relation = if embedded {
                        "go.embedding"
                    } else {
                        "go.type_set"
                    };
                    names.fact(relation, vec![Arg::Id(owner), Arg::Id(target)]);
                }
            }
            _ => {}
        }
    }
}

/// A method's receiver type arguments are its type parameters (go declares
/// none on the method); a malformed receiver skips it, as the entity walk does.
fn tsi_method(
    method: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let (Some(name_node), Some(receiver)) = (
        method.child_by_field_name("name"),
        method.child_by_field_name("receiver"),
    ) else {
        return;
    };
    let mut cursor = receiver.walk();
    let Some(declared) = receiver
        .children(&mut cursor)
        .find(|node| node.kind() == "parameter_declaration")
    else {
        return;
    };
    let Some(mut ty) = declared.child_by_field_name("type") else {
        return;
    };
    let mut arguments = None;
    loop {
        match ty.kind() {
            "pointer_type" => {
                let Some(inner) = ty.named_child(0) else {
                    return;
                };
                ty = inner;
            }
            "generic_type" => {
                arguments = ty.child_by_field_name("type_arguments");
                let Some(inner) = ty.child_by_field_name("type") else {
                    return;
                };
                ty = inner;
            }
            _ => break,
        }
    }
    let base = match ty.kind() {
        "type_identifier" => ty,
        "qualified_type" => match ty.child_by_field_name("name") {
            Some(name) => name,
            None => return,
        },
        _ => return,
    };
    let owner = names.named(strings, go_text(base, src), go_node_span(base));
    let callable = names.anonymous(go_node_span(name_node));
    let written = go_text(name_node, src);
    names.name(callable, written);
    names.fact("tsi.callable", vec![Arg::Id(callable)]);
    let mut scope = outer.clone();
    if let Some(arguments) = arguments {
        let mut position = 0i64;
        let mut terms = arguments.walk();
        for elem in arguments.named_children(&mut terms) {
            let Some(argument) = elem.named_child(0) else {
                continue;
            };
            if argument.kind() != "type_identifier" {
                continue;
            }
            let id = names.anonymous(go_node_span(argument));
            let text = go_text(argument, src);
            names.name(id, text);
            names.fact(
                "tsi.parameter",
                vec![
                    Arg::Id(id),
                    Arg::Id(callable),
                    Arg::Int(position),
                    Arg::Atom("unspecified".to_string()),
                ],
            );
            scope.insert(text.to_string(), id);
            position += 1;
        }
    }
    tsi_signature(callable, method, &scope, src, strings, names, state);
    let position = state.methods.entry(owner).or_insert(0);
    names.edge(owner, written, callable, *position);
    *position += 1;
}

/// Hands back the callable's id, which is what an owning type's member edge
/// names. A free function is ownerless and the id reaches nothing else.
fn tsi_callable(
    node: Node,
    name_node: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let callable = names.anonymous(go_node_span(name_node));
    names.name(callable, go_text(name_node, src));
    names.fact("tsi.callable", vec![Arg::Id(callable)]);
    let scope = tsi_generics(
        callable,
        node.child_by_field_name("type_parameters"),
        outer,
        src,
        strings,
        names,
        state,
    );
    tsi_signature(callable, node, &scope, src, strings, names, state);
    callable
}

/// One input per declared name (`a, b int` is two slots), receiver skipped;
/// one output per result slot; a variadic `...T` is written as `T`.
fn tsi_signature(
    callable: u32,
    node: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    if let Some(list) = node.child_by_field_name("parameters") {
        let mut position = 0i64;
        for (ty, count) in parameter_slots(list) {
            let target = tsi_type_id(ty, scope, src, strings, names, state);
            for _ in 0..count {
                names.fact(
                    "tsi.input",
                    vec![Arg::Id(callable), Arg::Int(position), Arg::Id(target)],
                );
                position += 1;
            }
        }
    }
    let Some(result) = node.child_by_field_name("result") else {
        return;
    };
    let slots = if result.kind() == "parameter_list" {
        parameter_slots(result)
    } else {
        vec![(result, 1)]
    };
    let mut position = 0i64;
    for (ty, count) in slots {
        let target = tsi_type_id(ty, scope, src, strings, names, state);
        for _ in 0..count {
            names.fact(
                "tsi.output",
                vec![Arg::Id(callable), Arg::Int(position), Arg::Id(target)],
            );
            position += 1;
        }
    }
}

/// Each parameter declaration's type with the number of slots it fills.
fn parameter_slots(list: Node) -> Vec<(Node, usize)> {
    let mut slots = Vec::new();
    let mut cursor = list.walk();
    for declared in list.children(&mut cursor) {
        if !matches!(
            declared.kind(),
            "parameter_declaration" | "variadic_parameter_declaration"
        ) {
            continue;
        }
        let Some(ty) = declared.child_by_field_name("type") else {
            continue;
        };
        let count = field_children(declared, "name").len().max(1);
        slots.push((ty, count));
    }
    slots
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
    if node.kind() == "generic_type" {
        tsi_application(id, node, scope, src, strings, names, state);
    }
    id
}

/// The application a written `Name[Args]` states, wherever written: the
/// callee is the name with its arguments dropped, once per written text.
fn tsi_application(
    result: u32,
    node: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let (Some(head), Some(arguments)) = (
        node.child_by_field_name("type"),
        node.child_by_field_name("type_arguments"),
    ) else {
        return;
    };
    if !state.called.insert(result) {
        return;
    }
    let callee = tsi_type_id(head, scope, src, strings, names, state);
    let list = names.bare_id();
    names.fact(
        "tsi.called",
        vec![Arg::Id(result), Arg::Id(callee), Arg::Id(list)],
    );
    let mut position = 0i64;
    let mut cursor = arguments.walk();
    for elem in arguments.named_children(&mut cursor) {
        let Some(argument) = elem.named_child(0) else {
            continue;
        };
        let target = tsi_type_id(argument, scope, src, strings, names, state);
        names.fact(
            "tsi.argument",
            vec![Arg::Id(list), Arg::Int(position), Arg::Id(target)],
        );
        position += 1;
    }
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

fn last_segment_text(node: Node, src: &[u8]) -> String {
    let node = strip_type(node);
    match node.kind() {
        "qualified_type" | "generic_type" => node
            .child_by_field_name(if node.kind() == "qualified_type" {
                "name"
            } else {
                "type"
            })
            .map(|inner| last_segment_text(inner, src))
            .unwrap_or_else(|| go_text(node, src).to_string()),
        _ => go_text(node, src).to_string(),
    }
}

/// Every child under a repeated field name, in source order.
fn field_children<'tree>(node: Node<'tree>, field: &str) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    let found: Vec<Node<'tree>> = node.children_by_field_name(field, &mut cursor).collect();
    found
}
