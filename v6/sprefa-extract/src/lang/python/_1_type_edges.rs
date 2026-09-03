//! The python syntax tier's TSI rows under `--witness --family type`: the
//! twin of `go_type_edges.rs` `tsi_rows`, beside the v5 entity port in `_0_source.rs`.

use std::collections::{BTreeMap, BTreeSet};

use tree_sitter::Node;

use crate::family::TypeF;
use crate::rows::FamilyBundle;
use crate::shape::{Span, Strings};
use crate::tsi::Arg;
use crate::types::{span_arg, TsiNames};

use super::_0_source::{node_span, py_text};

/// Type-parameter names in scope, innermost declaration last.
type TsiScope = BTreeMap<String, u32>;

/// Per-file bookkeeping: applications written, primitive ids, each owner's
/// next method position, the module's class names and its `TypeVar` names.
#[derive(Default)]
struct TsiState {
    called: BTreeSet<u32>,
    classes: BTreeMap<&'static str, u32>,
    methods: BTreeMap<u32, i64>,
    declared: BTreeSet<String>,
    typevars: BTreeSet<String>,
}

/// The builtin type names an annotation can spell without an import. `None`
/// is the annotation form of the `NoneType` class.
const PRIMITIVE_CLASSES: &[&str] = &[
    "int", "str", "float", "bool", "bytes", "complex", "object", "None",
];

/// Subscript heads whose arguments are the positions of an anonymous product.
const TUPLE_HEADS: &[&str] = &["tuple", "Tuple", "typing.Tuple"];

/// Base-class heads whose arguments declare the class's type parameters.
const PARAMETER_HEADS: &[&str] = &["Generic", "typing.Generic", "Protocol", "typing.Protocol"];

/// Builtin generic heads: a module-level `X = head[...]` is an alias.
const BUILTIN_GENERICS: &[&str] = &["list", "dict", "set", "frozenset", "tuple", "type"];

/// Module-level statements and class bodies only: a declaration inside a def
/// body is the checker's row.
pub(super) fn tsi_rows(
    root: Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let span = crate::trace::phase_span("python", crate::trace::Phase::TsiSyntax);
    let _entered = span.enter();
    let mut names = TsiNames::new("python");
    let outer = TsiScope::new();
    let mut state = TsiState::default();
    predeclare(root, src, strings, &mut names, &mut state);
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        tsi_statement(child, &outer, src, strings, &mut names, &mut state);
    }
    sink.aux.tsi = names.into_facts();
    crate::trace::record_phase(&span, 0, sink.aux.tsi.len() as u64, 1);
}

/// A class id origins at its own name even when referenced earlier, so every
/// class name is minted first; a `Name = TypeVar(...)` is kept for the defs.
fn predeclare(
    root: Node,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let target = unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                if let Some(name_node) = target.child_by_field_name("name") {
                    let text = py_text(name_node, src);
                    if !PRIMITIVE_CLASSES.contains(&text) {
                        names.named(strings, text, node_span(name_node));
                        state.declared.insert(text.to_string());
                    }
                }
            }
            "expression_statement" => {
                for assignment in named_children(target) {
                    if assignment.kind() != "assignment" {
                        continue;
                    }
                    let (Some(left), Some(right)) = (
                        assignment.child_by_field_name("left"),
                        assignment.child_by_field_name("right"),
                    ) else {
                        continue;
                    };
                    if left.kind() != "identifier" || right.kind() != "call" {
                        continue;
                    }
                    let callee = right
                        .child_by_field_name("function")
                        .map(|function| last_segment_text(function, src))
                        .unwrap_or_default();
                    if callee == "TypeVar" {
                        state.typevars.insert(py_text(left, src).to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

fn tsi_statement(
    node: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let node = unwrap_decorated(node);
    match node.kind() {
        "class_definition" => tsi_class(node, outer, src, strings, names, state),
        "function_definition" => {
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            tsi_callable(node, name_node, outer, false, src, strings, names, state);
        }
        "type_alias_statement" => tsi_type_alias(node, outer, src, strings, names, state),
        "expression_statement" => {
            for assignment in named_children(node) {
                if assignment.kind() == "assignment" {
                    tsi_module_assignment(assignment, outer, src, strings, names, state);
                }
            }
        }
        _ => {}
    }
}

/// `x: T = ...` carries the written type at the identifier; `X = <type>` with
/// no annotation is an alias when its right side is shaped like a type.
fn tsi_module_assignment(
    assignment: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Some(left) = assignment.child_by_field_name("left") else {
        return;
    };
    if left.kind() != "identifier" {
        return;
    }
    if let Some(ty) = assignment.child_by_field_name("type") {
        let target = tsi_type_id(ty, outer, src, strings, names, state);
        names.fact(
            "tsi.has_type",
            vec![span_arg(node_span(left)), Arg::Id(target)],
        );
        return;
    }
    let Some(right) = assignment.child_by_field_name("right") else {
        return;
    };
    if !alias_shaped(right, src, state) {
        return;
    }
    let symbol = names.bare_id();
    names.fact("tsi.symbol", vec![Arg::Id(symbol)]);
    names.name(symbol, py_text(left, src));
    let target = tsi_type_id(right, outer, src, strings, names, state);
    names.fact("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(target)]);
}

/// A right side a parse can call a type: a class or builtin name, a dotted or
/// subscripted name with a capitalised or builtin-generic last segment, or `|`s of those.
fn alias_shaped(node: Node, src: &[u8], state: &TsiState) -> bool {
    match node.kind() {
        "identifier" => {
            let text = py_text(node, src);
            PRIMITIVE_CLASSES.contains(&text) || state.declared.contains(text)
        }
        "none" => true,
        "attribute" => capitalised(&last_segment_text(node, src)),
        "subscript" => {
            let Some(head) = node.child_by_field_name("value") else {
                return false;
            };
            let last = last_segment_text(head, src);
            BUILTIN_GENERICS.contains(&last.as_str()) || capitalised(&last)
        }
        "binary_operator" => {
            let (Some(left), Some(right), Some(operator)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
                node.child_by_field_name("operator"),
            ) else {
                return false;
            };
            py_text(operator, src) == "|"
                && alias_shaped(left, src, state)
                && alias_shaped(right, src, state)
        }
        "parenthesized_expression" => node
            .named_child(0)
            .map(|inner| alias_shaped(inner, src, state))
            .unwrap_or(false),
        _ => false,
    }
}

fn capitalised(text: &str) -> bool {
    text.chars().next().map(char::is_uppercase).unwrap_or(false)
}

/// A class is a product: an edge per base under its last segment, then per
/// annotated field under its name; methods count their own positions.
fn tsi_class(
    class: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Some(name_node) = class.child_by_field_name("name") else {
        return;
    };
    let owner = names.named(strings, py_text(name_node, src), node_span(name_node));
    names.fact("tsi.product", vec![Arg::Id(owner)]);
    let mut scope = outer.clone();
    let mut parameter_position = 0i64;
    tsi_generics(
        owner,
        class.child_by_field_name("type_parameters"),
        &mut scope,
        &mut parameter_position,
        src,
        strings,
        names,
        state,
    );
    let mut position = 0i64;
    if let Some(bases) = class.child_by_field_name("superclasses") {
        for base in named_children(bases) {
            if base.kind() == "keyword_argument" {
                continue;
            }
            if let Some((head, arguments)) = application_parts(base) {
                let head_text = py_text(head, src);
                if PARAMETER_HEADS.contains(&head_text) {
                    for argument in arguments {
                        declare_parameter(
                            argument,
                            owner,
                            &mut scope,
                            &mut parameter_position,
                            src,
                            strings,
                            names,
                            state,
                        );
                    }
                    continue;
                }
            }
            let target = tsi_type_id(base, &scope, src, strings, names, state);
            let label = last_segment_text(base, src);
            names.edge(owner, &label, target, position);
            position += 1;
        }
    }
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    for statement in named_children(body) {
        let statement = unwrap_decorated(statement);
        match statement.kind() {
            "expression_statement" => {
                for assignment in named_children(statement) {
                    if assignment.kind() != "assignment" {
                        continue;
                    }
                    let (Some(left), Some(ty)) = (
                        assignment.child_by_field_name("left"),
                        assignment.child_by_field_name("type"),
                    ) else {
                        continue;
                    };
                    if left.kind() != "identifier" {
                        continue;
                    }
                    let target = tsi_type_id(ty, &scope, src, strings, names, state);
                    names.edge(owner, py_text(left, src), target, position);
                    names.fact(
                        "tsi.has_type",
                        vec![span_arg(node_span(left)), Arg::Id(target)],
                    );
                    position += 1;
                }
            }
            "function_definition" => {
                let Some(method_name) = statement.child_by_field_name("name") else {
                    continue;
                };
                let callable = tsi_callable(
                    statement,
                    method_name,
                    &scope,
                    true,
                    src,
                    strings,
                    names,
                    state,
                );
                let method_position = state.methods.entry(owner).or_insert(0);
                names.edge(owner, py_text(method_name, src), callable, *method_position);
                *method_position += 1;
            }
            _ => {}
        }
    }
}

/// PEP 695 `[T, U: Bound, *Ts]`: one `tsi.parameter` per declared name plus
/// a `bound` edge for a constrained one.
fn tsi_generics(
    owner: u32,
    list: Option<Node>,
    scope: &mut TsiScope,
    position: &mut i64,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let Some(list) = list else {
        return;
    };
    for declared in named_children(list) {
        declare_parameter(declared, owner, scope, position, src, strings, names, state);
    }
}

/// The name under a parameter declaration: `T`, `T: Bound`, `*Ts`, or a
/// `type` wrapper around one of those. A non-name declares nothing.
fn declare_parameter(
    declared: Node,
    owner: u32,
    scope: &mut TsiScope,
    position: &mut i64,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let declared = unwrap_type(declared);
    let (name_node, bound) = match declared.kind() {
        "identifier" => (declared, None),
        "constrained_type" => {
            let terms = named_children(declared);
            let Some(first) = terms.first().map(|node| unwrap_type(*node)) else {
                return;
            };
            if first.kind() != "identifier" {
                return;
            }
            (first, terms.get(1).copied())
        }
        "splat_type" => match declared.named_child(0) {
            Some(inner) if inner.kind() == "identifier" => (inner, None),
            _ => return,
        },
        _ => return,
    };
    let id = names.anonymous(node_span(name_node));
    let written = py_text(name_node, src);
    names.name(id, written);
    names.fact(
        "tsi.parameter",
        vec![
            Arg::Id(id),
            Arg::Id(owner),
            Arg::Int(*position),
            Arg::Atom("unspecified".to_string()),
        ],
    );
    if let Some(bound) = bound {
        let target = tsi_type_id(bound, scope, src, strings, names, state);
        names.edge(id, "bound", target, 0);
    }
    scope.insert(written.to_string(), id);
    *position += 1;
}

/// `type X = T` and `type X[T] = ...`: a symbol that denotes the written type.
fn tsi_type_alias(
    statement: Node,
    outer: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
    let (Some(left), Some(right)) = (
        statement.child_by_field_name("left"),
        statement.child_by_field_name("right"),
    ) else {
        return;
    };
    let left = unwrap_type(left);
    let (name_node, parameters) = match left.kind() {
        "identifier" => (left, None),
        "generic_type" => {
            let parts = named_children(left);
            match parts.first() {
                Some(name) if name.kind() == "identifier" => (*name, parts.get(1).copied()),
                _ => return,
            }
        }
        _ => return,
    };
    let symbol = names.bare_id();
    names.fact("tsi.symbol", vec![Arg::Id(symbol)]);
    names.name(symbol, py_text(name_node, src));
    let mut scope = outer.clone();
    let mut position = 0i64;
    tsi_generics(
        symbol,
        parameters,
        &mut scope,
        &mut position,
        src,
        strings,
        names,
        state,
    );
    let target = tsi_type_id(right, &scope, src, strings, names, state);
    names.fact("tsi.denotes", vec![Arg::Id(symbol), Arg::Id(target)]);
}

/// Hands back the callable's id, which is what an owning class's member edge
/// names. A module-level def is ownerless and the id reaches nothing else.
fn tsi_callable(
    def: Node,
    name_node: Node,
    outer: &TsiScope,
    method: bool,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let callable = names.anonymous(node_span(name_node));
    names.name(callable, py_text(name_node, src));
    names.fact("tsi.callable", vec![Arg::Id(callable)]);
    let mut scope = outer.clone();
    let mut parameter_position = 0i64;
    tsi_generics(
        callable,
        def.child_by_field_name("type_parameters"),
        &mut scope,
        &mut parameter_position,
        src,
        strings,
        names,
        state,
    );
    let parameters = def.child_by_field_name("parameters");
    let return_type = def.child_by_field_name("return_type");
    let mut referenced = Vec::new();
    if let Some(parameters) = parameters {
        for parameter in named_children(parameters) {
            if let Some(ty) = parameter.child_by_field_name("type") {
                typevar_references(ty, src, state, &scope, &mut referenced);
            }
        }
    }
    if let Some(ty) = return_type {
        typevar_references(ty, src, state, &scope, &mut referenced);
    }
    for typevar in referenced {
        declare_parameter(
            typevar,
            callable,
            &mut scope,
            &mut parameter_position,
            src,
            strings,
            names,
            state,
        );
    }
    if let Some(parameters) = parameters {
        let mut slot = 0i64;
        for parameter in named_children(parameters) {
            if matches!(
                parameter.kind(),
                "positional_separator" | "keyword_separator"
            ) {
                continue;
            }
            if method
                && slot == 0
                && parameter.kind() == "identifier"
                && matches!(py_text(parameter, src), "self" | "cls")
            {
                continue;
            }
            if let Some(ty) = parameter.child_by_field_name("type") {
                let target = tsi_type_id(ty, &scope, src, strings, names, state);
                names.fact(
                    "tsi.input",
                    vec![Arg::Id(callable), Arg::Int(slot), Arg::Id(target)],
                );
                if let Some(declared) = parameter_name(parameter) {
                    names.fact(
                        "tsi.has_type",
                        vec![span_arg(node_span(declared)), Arg::Id(target)],
                    );
                }
            }
            slot += 1;
        }
    }
    if let Some(ty) = return_type {
        let target = tsi_type_id(ty, &scope, src, strings, names, state);
        names.fact(
            "tsi.output",
            vec![Arg::Id(callable), Arg::Int(0), Arg::Id(target)],
        );
    }
    callable
}

/// The identifiers under a written type that name a module `TypeVar` not
/// already in scope, in written order, once each.
fn typevar_references<'tree>(
    node: Node<'tree>,
    src: &[u8],
    state: &TsiState,
    scope: &TsiScope,
    found: &mut Vec<Node<'tree>>,
) {
    if node.kind() == "identifier" {
        let text = py_text(node, src);
        if state.typevars.contains(text)
            && !scope.contains_key(text)
            && !found.iter().any(|seen| py_text(*seen, src) == text)
        {
            found.push(node);
        }
        return;
    }
    if node.kind() == "attribute" {
        return;
    }
    for child in named_children(node) {
        typevar_references(child, src, state, scope, found);
    }
}

/// The identifier a parameter declares: bare, defaulted, `*rest` or `**options`.
fn parameter_name(parameter: Node) -> Option<Node> {
    if let Some(name) = parameter.child_by_field_name("name") {
        return Some(name);
    }
    for child in named_children(parameter) {
        match child.kind() {
            "identifier" => return Some(child),
            "list_splat_pattern" | "dictionary_splat_pattern" => {
                return child
                    .named_child(0)
                    .filter(|inner| inner.kind() == "identifier")
            }
            _ => {}
        }
    }
    None
}

/// One id per written text except a scoped parameter or a primitive; `tuple[...]`
/// is an anonymous product, `A | B` an anonymous sum, a string the text it quotes.
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
        "none" => tsi_primitive_id("None", names, state),
        "string" => {
            let content = named_children(node)
                .into_iter()
                .find(|child| child.kind() == "string_content")
                .unwrap_or(node);
            names.named(strings, py_text(content, src), node_span(content))
        }
        "union_type" => tsi_sum_id(
            node,
            named_children(node),
            scope,
            src,
            strings,
            names,
            state,
        ),
        "binary_operator" if is_union_operator(node, src) => {
            let mut members = Vec::new();
            union_members(node, src, &mut members);
            tsi_sum_id(node, members, scope, src, strings, names, state)
        }
        "splat_type" | "constrained_type" => match node.named_child(0) {
            Some(inner) => tsi_type_id(inner, scope, src, strings, names, state),
            None => names.named(strings, py_text(node, src), node_span(node)),
        },
        _ => {
            let text = py_text(node, src);
            if let Some(&id) = scope.get(text) {
                return id;
            }
            if let Some(class) = PRIMITIVE_CLASSES.iter().find(|class| **class == text) {
                return tsi_primitive_id(class, names, state);
            }
            if let Some((head, arguments)) = application_parts(node) {
                if TUPLE_HEADS.contains(&py_text(head, src)) {
                    return tsi_tuple_id(node, arguments, scope, src, strings, names, state);
                }
                let id = names.named(strings, text, origin_span(node));
                tsi_application(id, head, arguments, scope, src, strings, names, state);
                return id;
            }
            names.named(strings, text, origin_span(node))
        }
    }
}

/// The application a written `Name[Args]` states, wherever written: the
/// callee is the head with its arguments dropped, once per written text.
fn tsi_application(
    result: u32,
    head: Node,
    arguments: Vec<Node>,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) {
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
    for argument in arguments {
        if argument.kind() == "ellipsis" {
            continue;
        }
        let target = tsi_type_id(argument, scope, src, strings, names, state);
        names.fact(
            "tsi.argument",
            vec![Arg::Id(list), Arg::Int(position), Arg::Id(target)],
        );
        position += 1;
    }
}

/// A tuple is structural, so its identity is its ordered edges rather than
/// its text, and every occurrence takes a fresh id; `...` fills no position.
fn tsi_tuple_id(
    node: Node,
    arguments: Vec<Node>,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let id = names.anonymous(node_span(node));
    names.name(id, py_text(node, src));
    names.fact("tsi.product", vec![Arg::Id(id)]);
    let mut position = 0i64;
    for element in arguments {
        if element.kind() == "ellipsis" {
            continue;
        }
        let target = tsi_type_id(element, scope, src, strings, names, state);
        names.edge(id, &position.to_string(), target, position);
        position += 1;
    }
    id
}

/// `A | B` is a sum whose edges are labelled by each member's written text.
fn tsi_sum_id(
    node: Node,
    members: Vec<Node>,
    scope: &TsiScope,
    src: &[u8],
    strings: &mut Strings,
    names: &mut TsiNames,
    state: &mut TsiState,
) -> u32 {
    let id = names.anonymous(node_span(node));
    names.name(id, py_text(node, src));
    names.fact("tsi.sum", vec![Arg::Id(id)]);
    for (position, member) in members.into_iter().enumerate() {
        let target = tsi_type_id(member, scope, src, strings, names, state);
        names.edge(id, py_text(member, src), target, position as i64);
    }
    id
}

fn is_union_operator(node: Node, src: &[u8]) -> bool {
    node.child_by_field_name("operator")
        .map(|operator| py_text(operator, src) == "|")
        .unwrap_or(false)
}

/// The leaves of a left-nested `A | B | C`, in written order.
fn union_members<'tree>(node: Node<'tree>, src: &[u8], members: &mut Vec<Node<'tree>>) {
    if node.kind() == "binary_operator" && is_union_operator(node, src) {
        if let Some(left) = node.child_by_field_name("left") {
            union_members(left, src, members);
        }
        if let Some(right) = node.child_by_field_name("right") {
            union_members(right, src, members);
        }
        return;
    }
    members.push(node);
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

/// The head and arguments of a written `Head[Args]`, under either grammar
/// spelling: `subscript` in expression position, `generic_type` in annotation position.
fn application_parts(node: Node) -> Option<(Node, Vec<Node>)> {
    let node = unwrap_type(node);
    match node.kind() {
        "subscript" => {
            let head = node.child_by_field_name("value")?;
            let mut cursor = node.walk();
            let arguments: Vec<Node> = node
                .children_by_field_name("subscript", &mut cursor)
                .filter(|argument| argument.kind() != "slice")
                .collect();
            Some((head, arguments))
        }
        "generic_type" => {
            let parts = named_children(node);
            let head = *parts.first()?;
            let arguments = parts
                .get(1)
                .map(|list| named_children(*list))
                .unwrap_or_default();
            Some((head, arguments))
        }
        _ => None,
    }
}

/// `type` and parentheses wrap a written type without changing it.
fn unwrap_type(node: Node) -> Node {
    let mut node = node;
    loop {
        match node.kind() {
            "type" | "parenthesized_expression" => match node.named_child(0) {
                Some(inner) => node = inner,
                None => return node,
            },
            _ => return node,
        }
    }
}

fn unwrap_decorated(node: Node) -> Node {
    if node.kind() == "decorated_definition" {
        node.child_by_field_name("definition").unwrap_or(node)
    } else {
        node
    }
}

/// The last segment of a dotted or subscripted name is the written type; the
/// rest qualifies or applies it.
fn origin_span(node: Node) -> Span {
    let node = unwrap_type(node);
    match node.kind() {
        "attribute" => node
            .child_by_field_name("attribute")
            .map(node_span)
            .unwrap_or_else(|| node_span(node)),
        "subscript" => node
            .child_by_field_name("value")
            .map(origin_span)
            .unwrap_or_else(|| node_span(node)),
        "generic_type" => node
            .named_child(0)
            .map(origin_span)
            .unwrap_or_else(|| node_span(node)),
        _ => node_span(node),
    }
}

fn last_segment_text(node: Node, src: &[u8]) -> String {
    let node = unwrap_type(node);
    match node.kind() {
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|inner| last_segment_text(inner, src))
            .unwrap_or_else(|| py_text(node, src).to_string()),
        "subscript" => node
            .child_by_field_name("value")
            .map(|inner| last_segment_text(inner, src))
            .unwrap_or_else(|| py_text(node, src).to_string()),
        "generic_type" => node
            .named_child(0)
            .map(|inner| last_segment_text(inner, src))
            .unwrap_or_else(|| py_text(node, src).to_string()),
        _ => py_text(node, src).to_string(),
    }
}

/// Every named child in source order.
fn named_children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    let found: Vec<Node> = node.named_children(&mut cursor).collect();
    found
}
