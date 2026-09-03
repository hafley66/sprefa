//! The kotlin syntax tier's TSI rows under `--witness --family type`: the
//! twin of `python/_1_type_edges.rs` `tsi_rows`, beside the v5 entity port in `kotlin.rs`.

use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::Node;

use crate::family::TypeF;
use crate::rows::FamilyBundle;
use crate::shape::{Span, Strings};
use crate::tsi::Arg;
use crate::types::{span_arg, TsiNames};

use super::kotlin::{kt_text, node_span};

/// Type-parameter names in scope, innermost declaration last.
type TsiScope = BTreeMap<String, u32>;

/// A declared class or object with the last segments of its written
/// supertypes: a sealed owner finds its direct subclasses here.
struct Declared {
    name: String,
    span: Span,
    supertypes: Vec<String>,
}

/// Per-file bookkeeping: applications written, primitive ids, each owner's
/// next method position, and every class declared in the file in order.
#[derive(Default)]
struct TsiState {
    called: BTreeSet<u32>,
    classes: BTreeMap<&'static str, u32>,
    methods: BTreeMap<u32, i64>,
    declared: Vec<Declared>,
}

/// The type names kotlin declares without an import as primitives. `null` is
/// the second arm a nullable type states, spelled as its literal.
const PRIMITIVE_CLASSES: &[&str] = &[
    "Int", "Long", "Short", "Byte", "Double", "Float", "Boolean", "Char", "String", "Unit",
    "Nothing", "Any", "null",
];

/// The node kinds a written type takes.
const TYPE_KINDS: &[&str] = &[
    "user_type",
    "nullable_type",
    "function_type",
    "parenthesized_type",
    "not_nullable_type",
];

/// Top-level declarations and class bodies only: a declaration inside a
/// function body is the checker's row.
pub(super) fn tsi_rows(
    root: Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let span = crate::trace::phase_span("kotlin", crate::trace::Phase::TsiSyntax);
    let _entered = span.enter();
    let mut names = TsiNames::new("kotlin");
    let outer = TsiScope::new();
    let mut state = TsiState::default();
    predeclare(root, src, strings, &mut names, &mut state);
    for child in named_children(root) {
        tsi_declaration(child, &outer, src, strings, &mut names, &mut state);
    }
    sink.aux.tsi = names.into_facts();
    crate::trace::record_phase(&span, 0, sink.aux.tsi.len() as u64, 1);
}

/// A class id origins at its own name even when referenced earlier, so every
/// class name is minted first, nested bodies included.
fn predeclare(
    node: Node,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    for child in named_children(node) {
        match child.kind() {
            "class_declaration" | "object_declaration" => {
                let Some(name_node) = first_child(child, "type_identifier") else {
                    continue;
                };
                let text = kt_text(name_node, src);
                if !PRIMITIVE_CLASSES.contains(&text) {
                    names.named(strings, text, node_span(name_node));
                }
                let supertypes = named_children(child)
                    .into_iter()
                    .filter(|part| part.kind() == "delegation_specifier")
                    .filter_map(|part| supertype_node(part))
                    .map(|written| last_segment_text(written, src))
                    .collect();
                state.declared.push(Declared {
                    name: text.to_string(),
                    span: node_span(name_node),
                    supertypes,
                });
                if let Some(body) = class_body(child) {
                    predeclare(body, src, strings, names, state);
                }
            }
            _ => {}
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
        "class_declaration" | "object_declaration" => {
            tsi_class(node, outer, src, strings, names, state);
        }
        "function_declaration" => {
            tsi_function(node, None, outer, src, strings, names, state);
        }
        "property_declaration" => {
            if let Some((name_node, ty)) = property_parts(node) {
                let target = tsi_type_id(ty, outer, src, strings, names, state);
                names.fact(
                    "tsi.has_type",
                    vec![span_arg(node_span(name_node)), Arg::Id(target)],
                );
            }
        }
        "type_alias" => tsi_type_alias(node, outer, src, strings, names, state),
        _ => {}
    }
}

/// A class is a product, a sealed or enum class a sum; edges: supertypes by
/// last segment, subclasses or entries, properties. Methods count their own positions.
fn tsi_class(
    class: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Some(name_node) = first_child(class, "type_identifier") else {
        return;
    };
    let owner_text = kt_text(name_node, src);
    let owner = names.named(strings, owner_text, node_span(name_node));
    let parts = named_children(class);
    let sealed = has_class_modifier(class, "sealed", src);
    let enum_class = first_child(class, "enum").is_some();
    if sealed || enum_class {
        names.fact("tsi.sum", vec![Arg::Id(owner)]);
    } else {
        names.fact("tsi.product", vec![Arg::Id(owner)]);
    }
    let scope = tsi_generics(
        owner,
        first_child(class, "type_parameters"),
        first_child(class, "type_constraints"),
        outer,
        src,
        strings,
        names,
        state,
    );
    let mut position = 0i64;
    for part in parts
        .iter()
        .filter(|part| part.kind() == "delegation_specifier")
    {
        let Some(written) = supertype_node(*part) else {
            continue;
        };
        let target = tsi_type_id(written, &scope, src, strings, names, state);
        let label = last_segment_text(written, src);
        names.edge(owner, &label, target, position);
        position += 1;
    }
    if sealed {
        let subclasses: Vec<(String, Span)> = state
            .declared
            .iter()
            .filter(|declared| declared.supertypes.iter().any(|name| name == owner_text))
            .map(|declared| (declared.name.clone(), declared.span))
            .collect();
        for (name, span) in subclasses {
            let target = names.named(strings, &name, span);
            names.edge(owner, &name, target, position);
            position += 1;
        }
    }
    if let Some(constructor) = first_child(class, "primary_constructor") {
        for parameter in named_children(constructor) {
            if parameter.kind() != "class_parameter"
                || first_child(parameter, "binding_pattern_kind").is_none()
            {
                continue;
            }
            let (Some(declared), Some(ty)) = (
                first_child(parameter, "simple_identifier"),
                type_child(parameter),
            ) else {
                continue;
            };
            let target = tsi_type_id(ty, &scope, src, strings, names, state);
            names.edge(owner, kt_text(declared, src), target, position);
            names.fact(
                "tsi.has_type",
                vec![span_arg(node_span(declared)), Arg::Id(target)],
            );
            position += 1;
        }
    }
    let Some(body) = class_body(class) else {
        return;
    };
    for member in named_children(body) {
        match member.kind() {
            "enum_entry" => {
                let Some(declared) = first_child(member, "simple_identifier") else {
                    continue;
                };
                let entry = kt_text(declared, src);
                let written = format!("{owner_text}.{entry}");
                let target = names.named(strings, &written, node_span(declared));
                names.edge(owner, entry, target, position);
                position += 1;
            }
            "property_declaration" => {
                let Some((declared, ty)) = property_parts(member) else {
                    continue;
                };
                let target = tsi_type_id(ty, &scope, src, strings, names, state);
                names.edge(owner, kt_text(declared, src), target, position);
                names.fact(
                    "tsi.has_type",
                    vec![span_arg(node_span(declared)), Arg::Id(target)],
                );
                position += 1;
            }
            "function_declaration" => {
                tsi_function(member, Some(owner), &scope, src, strings, names, state);
            }
            "class_declaration" | "object_declaration" => {
                tsi_class(member, &scope, src, strings, names, state);
            }
            _ => {}
        }
    }
}

/// One `tsi.parameter` per declared name and a `bound` edge per constraint
/// term; the name enters scope before its bound, so `T : Comparable<T>` reaches itself.
fn tsi_generics(
    owner: u32,
    list: Option<Node>,
    constraints: Option<Node>,
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
    let mut ranks: BTreeMap<String, (u32, i64)> = BTreeMap::new();
    for declared in named_children(list) {
        if declared.kind() != "type_parameter" {
            continue;
        }
        let Some(name_node) = first_child(declared, "type_identifier") else {
            continue;
        };
        let id = names.anonymous(node_span(name_node));
        let written = kt_text(name_node, src);
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
        scope.insert(written.to_string(), id);
        let mut rank = 0i64;
        if let Some(bound) = type_child(declared) {
            let target = tsi_type_id(bound, &scope, src, strings, names, state);
            names.edge(id, "bound", target, rank);
            rank += 1;
        }
        ranks.insert(written.to_string(), (id, rank));
        position += 1;
    }
    let Some(constraints) = constraints else {
        return scope;
    };
    for constraint in named_children(constraints) {
        if constraint.kind() != "type_constraint" {
            continue;
        }
        let (Some(name_node), Some(bound)) = (
            first_child(constraint, "type_identifier"),
            type_child(constraint),
        ) else {
            continue;
        };
        let Some((id, rank)) = ranks.get_mut(kt_text(name_node, src)) else {
            continue;
        };
        let target = tsi_type_id(bound, &scope, src, strings, names, state);
        names.edge(*id, "bound", target, *rank);
        *rank += 1;
    }
    scope
}

/// `typealias X<T> = Written`: a symbol that denotes the written type.
fn tsi_type_alias(
    alias: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let (Some(name_node), Some(ty)) = (first_child(alias, "type_identifier"), type_child(alias))
    else {
        return;
    };
    let symbol = names.bare_id();
    names.fact("tsi.symbol", vec![Arg::Id(symbol)]);
    names.name(symbol, kt_text(name_node, src));
    let scope = tsi_generics(
        symbol,
        first_child(alias, "type_parameters"),
        None,
        outer,
        src,
        strings,
        names,
        state,
    );
    let target = tsi_type_id(ty, &scope, src, strings, names, state);
    names.fact("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(target)]);
}

/// A member's owner is its class, an extension's the receiver head (as go's
/// method receiver names the owner); the receiver fills no slot.
fn tsi_function(
    function: Node,
    member_of: Option<u32>,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Some(name_node) = first_child(function, "simple_identifier") else {
        return;
    };
    let written = kt_text(name_node, src);
    let callable = names.anonymous(node_span(name_node));
    names.name(callable, written);
    names.fact("tsi.callable", vec![Arg::Id(callable)]);
    let scope = tsi_generics(
        callable,
        first_child(function, "type_parameters"),
        first_child(function, "type_constraints"),
        outer,
        src,
        strings,
        names,
        state,
    );
    let owner = member_of.or_else(|| {
        let receiver = function.child_by_field_name("receiver")?;
        receiver_owner(receiver, &scope, src, strings, names, state)
    });
    if let Some(parameters) = first_child(function, "function_value_parameters") {
        let mut slot = 0i64;
        for parameter in named_children(parameters) {
            if parameter.kind() != "parameter" {
                continue;
            }
            if let Some(ty) = type_child(parameter) {
                let target = tsi_type_id(ty, &scope, src, strings, names, state);
                names.fact(
                    "tsi.input",
                    vec![Arg::Id(callable), Arg::Int(slot), Arg::Id(target)],
                );
                if let Some(declared) = first_child(parameter, "simple_identifier") {
                    names.fact(
                        "tsi.has_type",
                        vec![span_arg(node_span(declared)), Arg::Id(target)],
                    );
                }
            }
            slot += 1;
        }
    }
    if let Some(ty) = return_type(function) {
        let target = tsi_type_id(ty, &scope, src, strings, names, state);
        names.fact(
            "tsi.output",
            vec![Arg::Id(callable), Arg::Int(0), Arg::Id(target)],
        );
    }
    let Some(owner) = owner else {
        return;
    };
    let position = state.methods.entry(owner).or_insert(0);
    names.edge(owner, written, callable, *position);
    *position += 1;
}

/// The head of a receiver type, its arguments and `?` dropped: `List<T>.` and
/// `Base?.` both name the declared type. A scoped parameter receiver owns nothing.
fn receiver_owner(
    receiver: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> Option<u32> {
    let mut written = type_child(receiver)?;
    loop {
        match written.kind() {
            "nullable_type" | "parenthesized_type" => written = type_child(written)?,
            _ => break,
        }
    }
    if written.kind() != "user_type" {
        return None;
    }
    let (head_text, head_span) = user_type_head(written, src);
    if scope.contains_key(head_text.as_str()) {
        return None;
    }
    Some(tsi_head_id(&head_text, head_span, names, strings, state))
}

/// The written type after the parameter list: `fun f(): T`.
fn return_type(function: Node) -> Option<Node> {
    let mut past_parameters = false;
    for child in named_children(function) {
        if child.kind() == "function_value_parameters" {
            past_parameters = true;
            continue;
        }
        if past_parameters && TYPE_KINDS.contains(&child.kind()) {
            return Some(child);
        }
    }
    None
}

/// One id per written text except a scoped parameter or a primitive; `T?` is
/// an anonymous sum of `T` and `null`, `(A) -> B` an anonymous callable.
fn tsi_type_id(
    node: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let node = unwrap_type(node);
    match node.kind() {
        "nullable_type" => tsi_nullable_id(node, scope, src, strings, names, state),
        "function_type" => tsi_function_type_id(node, scope, src, strings, names, state),
        "user_type" => {
            let text = kt_text(node, src);
            if let Some(&id) = scope.get(text) {
                return id;
            }
            if let Some(class) = PRIMITIVE_CLASSES.iter().find(|class| **class == text) {
                return tsi_primitive_id(class, names, state);
            }
            let (head_text, head_span) = user_type_head(node, src);
            let id = names.named(strings, text, head_span);
            if let Some(arguments) = last_child(node, "type_arguments") {
                tsi_application(
                    id, &head_text, head_span, arguments, scope, src, strings, names, state,
                );
            }
            id
        }
        _ => names.named(strings, kt_text(node, src), node_span(node)),
    }
}

/// The application a written `Name<Args>` states, wherever written: the
/// callee is the head with its arguments dropped, once per written text.
fn tsi_application(
    result: u32,
    head_text: &str,
    head_span: Span,
    arguments: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    if !state.called.insert(result) {
        return;
    }
    let callee = match scope.get(head_text) {
        Some(&id) => id,
        None => tsi_head_id(head_text, head_span, names, strings, state),
    };
    let list = names.bare_id();
    names.fact(
        "tsi.called",
        vec![Arg::Id(result), Arg::Id(callee), Arg::Id(list)],
    );
    let mut position = 0i64;
    for projection in named_children(arguments) {
        let Some(argument) = type_child(projection) else {
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

/// The id of a head spelled without arguments: a primitive by class, else one
/// per written text at the head's last segment.
fn tsi_head_id(
    head_text: &str,
    head_span: Span,
    names: &mut TsiNames,
    strings: &mut Strings,
    state: &mut TsiState,
) -> u32 {
    if let Some(class) = PRIMITIVE_CLASSES.iter().find(|class| **class == head_text) {
        return tsi_primitive_id(class, names, state);
    }
    names.named(strings, head_text, head_span)
}

/// `T?` is a sum whose arms are the written inner type and `null`, labelled
/// by their spellings; every occurrence takes a fresh id.
fn tsi_nullable_id(
    node: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let id = names.anonymous(node_span(node));
    names.name(id, kt_text(node, src));
    names.fact("tsi.sum", vec![Arg::Id(id)]);
    if let Some(inner) = type_child(node) {
        let target = tsi_type_id(inner, scope, src, strings, names, state);
        names.edge(id, kt_text(inner, src), target, 0);
    }
    let null = tsi_primitive_id("null", names, state);
    names.edge(id, "null", null, 1);
    id
}

/// `R.(A, b: B) -> C` is an anonymous callable: a receiver fills slot 0,
/// each parameter the next slot, the result the single output.
fn tsi_function_type_id(
    node: Node,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let id = names.anonymous(node_span(node));
    names.name(id, kt_text(node, src));
    names.fact("tsi.callable", vec![Arg::Id(id)]);
    let mut slot = 0i64;
    if let Some(receiver) = node.child_by_field_name("receiver") {
        if let Some(ty) = type_child(receiver) {
            let target = tsi_type_id(ty, scope, src, strings, names, state);
            names.fact(
                "tsi.input",
                vec![Arg::Id(id), Arg::Int(slot), Arg::Id(target)],
            );
            slot += 1;
        }
    }
    if let Some(parameters) = first_child(node, "function_type_parameters") {
        for parameter in named_children(parameters) {
            let ty = if parameter.kind() == "parameter" {
                type_child(parameter)
            } else if TYPE_KINDS.contains(&parameter.kind()) {
                Some(parameter)
            } else {
                None
            };
            let Some(ty) = ty else {
                continue;
            };
            let target = tsi_type_id(ty, scope, src, strings, names, state);
            names.fact(
                "tsi.input",
                vec![Arg::Id(id), Arg::Int(slot), Arg::Id(target)],
            );
            slot += 1;
        }
    }
    let mut past_parameters = false;
    for child in named_children(node) {
        if child.kind() == "function_type_parameters" {
            past_parameters = true;
            continue;
        }
        if past_parameters && TYPE_KINDS.contains(&child.kind()) {
            let target = tsi_type_id(child, scope, src, strings, names, state);
            names.fact(
                "tsi.output",
                vec![Arg::Id(id), Arg::Int(0), Arg::Id(target)],
            );
            break;
        }
    }
    id
}

/// A primitive is declared by the language, so it carries a class rather than
/// an origin: no range in this file declares it.
fn tsi_primitive_id(class: &'static str, names: &mut TsiNames, state: &mut TsiState) -> u32 {
    if let Some(&id) = state.classes.get(class) {
        return id;
    }
    let id = names.bare_id();
    names.fact("tsi.type", vec![Arg::Id(id)]);
    names.fact(
        "tsi.primitive",
        vec![Arg::Id(id), Arg::Atom(class.to_string())],
    );
    names.name(id, class);
    state.classes.insert(class, id);
    id
}

/// A user type's spelling with its last `<Args>` dropped, and the span of its
/// last segment: `kotlin.collections.List<String>` heads at `List`.
fn user_type_head(node: Node, src: &[u8]) -> (String, Span) {
    let segments: Vec<Node> = named_children(node)
        .into_iter()
        .filter(|part| part.kind() == "type_identifier")
        .collect();
    let last = segments.last().copied().unwrap_or(node);
    let text = match last_child(node, "type_arguments") {
        Some(arguments) => {
            let before = &src[node.start_byte()..arguments.start_byte()];
            let after = &src[arguments.end_byte()..node.end_byte()];
            format!(
                "{}{}",
                String::from_utf8_lossy(before),
                String::from_utf8_lossy(after)
            )
        }
        None => kt_text(node, src).to_string(),
    };
    (text, node_span(last))
}

/// The type a supertype clause writes: bare, constructed, or delegated.
fn supertype_node(specifier: Node) -> Option<Node> {
    for child in named_children(specifier) {
        if TYPE_KINDS.contains(&child.kind()) {
            return Some(child);
        }
        if matches!(
            child.kind(),
            "constructor_invocation" | "explicit_delegation"
        ) {
            return type_child(child);
        }
    }
    None
}

/// `val name: T` under a property: the declared identifier and its written
/// type; an untyped or destructured property declares nothing here.
fn property_parts(property: Node) -> Option<(Node, Node)> {
    let declaration = first_child(property, "variable_declaration")?;
    let name_node = first_child(declaration, "simple_identifier")?;
    let ty = type_child(declaration)?;
    Some((name_node, ty))
}

fn has_class_modifier(class: Node, modifier: &str, src: &[u8]) -> bool {
    first_child(class, "modifiers")
        .map(|modifiers| {
            named_children(modifiers)
                .into_iter()
                .any(|part| part.kind() == "class_modifier" && kt_text(part, src) == modifier)
        })
        .unwrap_or(false)
}

fn class_body(class: Node) -> Option<Node> {
    first_child(class, "class_body").or_else(|| first_child(class, "enum_class_body"))
}

/// Parentheses wrap a written type without changing it.
fn unwrap_type(node: Node) -> Node {
    let mut node = node;
    while node.kind() == "parenthesized_type" {
        match type_child(node) {
            Some(inner) => node = inner,
            None => return node,
        }
    }
    node
}

/// The last segment of a user type is the written type; the rest qualifies
/// or applies it.
fn last_segment_text(node: Node, src: &[u8]) -> String {
    let node = unwrap_type(node);
    match node.kind() {
        "user_type" => named_children(node)
            .into_iter()
            .filter(|part| part.kind() == "type_identifier")
            .last()
            .map(|part| kt_text(part, src).to_string())
            .unwrap_or_else(|| kt_text(node, src).to_string()),
        "nullable_type" => type_child(node)
            .map(|inner| last_segment_text(inner, src))
            .unwrap_or_else(|| kt_text(node, src).to_string()),
        _ => kt_text(node, src).to_string(),
    }
}

/// The first direct child that is a written type.
fn type_child(node: Node) -> Option<Node> {
    named_children(node)
        .into_iter()
        .find(|child| TYPE_KINDS.contains(&child.kind()))
}

/// The first direct child of `kind`, anonymous tokens included.
fn first_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn last_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .filter(|child| child.kind() == kind)
        .last();
    found
}

/// Every named child in source order.
fn named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    let found: Vec<Node> = node.named_children(&mut cursor).collect();
    found
}
