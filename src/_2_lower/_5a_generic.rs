//! A bind of a product application with `return` omitted, where `return` is a
//! constructor expression: the bind edge targets `application(Callee, Args)`
//! and check builds the instance from it.

use super::cx::Cx;
use super::express::{is_path_head, lower_expression};
use super::forms;
use crate::_6_eval::term::TermId;

pub fn generic_bind_edge(
    cx: &mut Cx,
    owner: TermId,
    name: TermId,
    target: TermId,
) -> Option<TermId> {
    let (_, args) = cx.u.functor(target)?;
    let (target_node, index) = (args[0], args[2]);
    let node = forms::node(cx.u, target_node)?;
    let application = generic_application(cx, owner, node.payload, 0)?;
    Some(cx.compound("pending_edge", vec![owner, name, application, index]))
}

/// Nesting of generic applications inside generic arguments.
const APPLICATION_DEPTH: usize = 16;

/// A product whose one `return` edge targets an `owner` expression.
pub fn is_generic_callable(cx: &mut Cx, callable: TermId) -> bool {
    let return_atom = cx.atom("return");
    let [return_index] = cx.edges.return_indices(callable, return_atom)[..] else {
        return false;
    };
    cx.edges
        .owned(callable)
        .into_iter()
        .find(|edge| edge.index == return_index && edge.name == return_atom)
        .and_then(|edge| cx.u.unary(edge.target, "target"))
        .is_some_and(|inner| cx.u.functor(inner).is_some_and(|(name, _)| name == "owner"))
}

/// `application(Callee, Args)` for a form that applies a generic product;
/// an argument that is itself such a form becomes a nested application.
fn generic_application(cx: &mut Cx, owner: TermId, payload: TermId, depth: usize) -> Option<TermId> {
    if depth >= APPLICATION_DEPTH {
        return None;
    }
    let items = forms::form(cx.u, payload)?;
    let head = forms::form_head_atom(cx.u, &items)?;
    if is_path_head(cx.u, head) {
        return None;
    }
    let found = cx.reservations.scoped(owner, head)?;
    let (found_target, found_kind) = (found.target, found.kind);
    let is_product = cx
        .u
        .functor_or_atom(found_kind)
        .is_some_and(|(kind, _)| kind == "product");
    let callable = cx.u.unary(found_target, "target").filter(|_| is_product)?;
    // A bounded application builds by rule, behind its checks.
    if !is_generic_callable(cx, callable) || !bounds(cx, callable).is_empty() {
        return None;
    }
    if items.len() as i64 != cx.edges.arity(callable) {
        return None;
    }
    let mut values = Vec::with_capacity(items.len() - 1);
    for item in &items[1..] {
        let payload = forms::node(cx.u, *item).map(|inner| inner.payload);
        if let Some(nested) = payload.and_then(|payload| generic_application(cx, owner, payload, depth + 1)) {
            values.push(nested);
            continue;
        }
        let lowered = lower_expression(cx, *item, owner);
        if !lowered.diagnostics.is_empty() || !lowered.goals.is_empty() {
            return None;
        }
        values.push(lowered.value);
    }
    let callee = cx.compound("name", vec![owner, head]);
    let list = cx.u.list(&values);
    Some(cx.compound("application", vec![callee, list]))
}

/// One bind of an eigo product other than `return`: a parameter when its
/// target is a type, fixed when its target is a value expression.
#[derive(Clone, Copy, Debug)]
pub struct EigoBind {
    pub label: TermId,
    pub target: TermId,
    pub index: i64,
    pub parameter: bool,
}

/// A product whose `return` edge names one of its own parameters: applying it
/// evaluates to that argument. Binds in index order, `return` left out. An
/// imported product carries each edge as declared and as resolved; a bind is
/// fixed when any of its rows is a value expression.
pub fn eigo(cx: &mut Cx, callable: TermId) -> Option<Vec<EigoBind>> {
    let return_atom = cx.atom("return");
    let edges: Vec<(TermId, TermId, i64)> = cx
        .edges
        .owned(callable)
        .into_iter()
        .map(|edge| (edge.name, edge.target, edge.index))
        .collect();
    let returned = edges
        .iter()
        .filter(|(name, _, _)| *name == return_atom)
        .find_map(|(_, target, _)| cx.u.args::<2>(*target, "name"))
        .map(|[_, label]| label)?;
    let mut binds: Vec<EigoBind> = Vec::new();
    for (label, target, index) in edges {
        if label == return_atom {
            continue;
        }
        let deferred = cx.u.unary(target, "deferred_expression").is_some();
        match binds.iter_mut().find(|bind| bind.index == index) {
            Some(bind) if deferred => {
                bind.target = target;
                bind.parameter = false;
            }
            Some(_) => {}
            None => binds.push(EigoBind {
                label,
                target,
                index,
                parameter: !deferred,
            }),
        }
    }
    binds.sort_by_key(|bind| bind.index);
    binds
        .iter()
        .any(|bind| bind.label == returned && bind.parameter)
        .then_some(binds)
}

/// The product a head node names: an atom in scope, or a dotted path through
/// module and product scopes.
pub fn head_product(cx: &mut Cx, head: TermId, owner: TermId) -> Option<TermId> {
    let segments = match super::express::path_segments(cx.u, head) {
        Some(segments) => segments,
        None => vec![head],
    };
    let mut scope = owner;
    for (position, segment) in segments.iter().enumerate() {
        let name = super::express::path_segment_atom(cx.u, *segment)?;
        let found = cx.reservations.scoped(scope, name)?;
        let (target, kind) = (found.target, found.kind);
        scope = cx.u.unary(target, "target")?;
        let last = position + 1 == segments.len();
        let is_product = cx
            .u
            .functor_or_atom(kind)
            .is_some_and(|(kind, _)| kind == "product");
        if last && !is_product {
            return None;
        }
    }
    Some(scope)
}

/// The rows a product-body application adds: the instance's edges, the rules
/// that copy fixed binds onto it, and their origins.
#[derive(Default)]
pub struct InstanceRows {
    pub edges: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub origins: Vec<TermId>,
}

/// `(Head Argument..)` as an item of `owner`'s body. Each argument is the
/// `ref(edge(Owner, Label))` of a bind already on `owner`, or the tuple of
/// such refs. The instance `application(Callee, Arguments)` gets one edge per
/// bind of the callee: a parameter edge targets its argument, a fixed edge is
/// copied from the callee's own row by one rule.
pub fn application_item(
    cx: &mut Cx,
    owner: TermId,
    node_id: TermId,
    target: TermId,
    rule_index: i64,
) -> Result<InstanceRows, TermId> {
    let Some([head, list]) = cx.u.args::<2>(target, "application") else {
        return Ok(InstanceRows::default());
    };
    let arguments = cx.u.as_list(list).unwrap_or_default();
    let head_name = super::express::path_segments(cx.u, head)
        .and_then(|segments| segments.last().copied())
        .unwrap_or(head);
    let head_name = super::express::path_segment_atom(cx.u, head_name).unwrap_or(head_name);
    let Some(callable) = head_product(cx, head, owner) else {
        let reason = cx.compound("undeclared_relation", vec![head_name]);
        return Err(cx.diagnostic(node_id, reason));
    };
    let Some(binds) = eigo(cx, callable) else {
        let shape = cx.atom("generic_application");
        let reason = cx.compound("not_built_yet", vec![shape]);
        return Err(cx.diagnostic(node_id, reason));
    };
    let parameters = binds.iter().filter(|bind| bind.parameter).count();
    if parameters != arguments.len() {
        let expected = cx.int(parameters as i64);
        let observed = cx.int(arguments.len() as i64);
        let reason = cx.compound("arity_mismatch", vec![head_name, expected, observed]);
        return Err(cx.diagnostic(node_id, reason));
    }
    let bounded = bounds(cx, callable);
    if !bounded.is_empty() {
        return bounded_item(cx, owner, node_id, callable, &binds, &arguments, &bounded, rule_index);
    }
    let plain: Vec<TermId> = arguments
        .iter()
        .map(|argument| cx.u.unary(*argument, "ref").unwrap_or(*argument))
        .collect();
    let plain = cx.u.list(&plain);
    let instance = cx.compound("application", vec![callable, plain]);
    let mut out = InstanceRows::default();
    let mut next_argument = arguments.iter();
    let colon = cx.atom(":");
    for (ordinal, bind) in binds.iter().enumerate() {
        let ordinal_term = cx.int(ordinal as i64);
        let edge_key = cx.compound("edge", vec![instance, bind.label, ordinal_term]);
        out.origins.push(cx.compound("origin", vec![edge_key, node_id]));
        if bind.parameter {
            let argument = *next_argument.next().unwrap_or(&head);
            let edge_target = match cx.u.unary(argument, "ref") {
                Some(edge) => cx.compound("target", vec![edge]),
                None => argument,
            };
            out.edges.push(cx.compound(
                "pending_edge",
                vec![instance, bind.label, edge_target, ordinal_term],
            ));
            continue;
        }
        out.edges.push(cx.compound(
            "pending_edge",
            vec![instance, bind.label, bind.target, ordinal_term],
        ));
        let key = cx.compound("derived_bind", vec![node_id]);
        let value = cx.compound("var", vec![key]);
        let relation = cx.compound("name", vec![owner, colon]);
        let label = cx.compound("const", vec![bind.label]);
        let instance_ref = cx.compound("ref", vec![instance]);
        let ordinal_const = cx.compound("const", vec![ordinal_term]);
        let head_arguments = cx.u.list(&[instance_ref, label, value, ordinal_const]);
        let rule_head = cx.compound("call", vec![relation, head_arguments]);
        let callable_ref = cx.compound("ref", vec![callable]);
        let index = cx.int(bind.index);
        let index_const = cx.compound("const", vec![index]);
        let goal_arguments = cx.u.list(&[callable_ref, label, value, index_const]);
        let goal_call = cx.compound("call", vec![relation, goal_arguments]);
        let positive = cx.atom("positive");
        let goal = cx.compound("pending_goal", vec![positive, goal_call]);
        let body = cx.u.list(&[goal]);
        out.rules.push(cx.compound("rule", vec![rule_head, body]));
        let rule_number = cx.int(rule_index + out.rules.len() as i64 - 1);
        let rule_key = cx.compound("rule", vec![rule_number]);
        out.origins.push(cx.compound("origin", vec![rule_key, node_id]));
        let zero = cx.int(0);
        let goal_key = cx.compound("goal", vec![rule_number, zero]);
        out.origins.push(cx.compound("origin", vec![goal_key, node_id]));
    }
    Ok(out)
}

/// A parameter whose target is a type: the argument in `column` is checked against `bound`.
#[derive(Clone, Copy, Debug)]
pub struct Bound {
    pub label: TermId,
    pub bound: TermId,
    pub column: usize,
}

/// Targets that leave a parameter unbounded; a primitive parameter takes a value.
const UNBOUNDED: [&str; 6] = ["type", "any", "int", "float", "bool", "str"];

/// The bounded parameters of a generic callable, in column order.
pub fn bounds(cx: &mut Cx, callable: TermId) -> Vec<Bound> {
    let parameters: Vec<(TermId, TermId)> = match eigo(cx, callable) {
        Some(binds) => binds
            .iter()
            .filter(|bind| bind.parameter)
            .map(|bind| (bind.label, bind.target))
            .collect(),
        None if is_generic_callable(cx, callable) => {
            let return_atom = cx.atom("return");
            let mut labelled: Vec<(i64, TermId, TermId)> = cx
                .edges
                .owned(callable)
                .into_iter()
                .filter(|edge| edge.name != return_atom)
                .map(|edge| (edge.index, edge.name, edge.target))
                .collect();
            labelled.sort_by_key(|(index, _, _)| *index);
            labelled.dedup_by_key(|(index, _, _)| *index);
            labelled
                .into_iter()
                .map(|(_, label, target)| (label, target))
                .collect()
        }
        None => return Vec::new(),
    };
    parameters
        .into_iter()
        .enumerate()
        .filter_map(|(column, (label, target))| {
            bound_term(cx, target).map(|bound| Bound {
                label,
                bound,
                column,
            })
        })
        .collect()
}

/// The rule argument naming a parameter's bound; `None` when it is unbounded.
fn bound_term(cx: &mut Cx, target: TermId) -> Option<TermId> {
    if let Some([_, name]) = cx.u.args::<2>(target, "name") {
        let text = cx.u.functor_or_atom(name).map(|(n, _)| n.to_string())?;
        return (!UNBOUNDED.contains(&text.as_str())).then_some(target);
    }
    let inner = cx
        .u
        .unary(target, "target")
        .or_else(|| cx.u.unary(target, "ref"))?;
    let primitive = cx
        .u
        .functor(inner)
        .is_some_and(|(name, args)| name == "primitive" && args.len() == 1);
    (!primitive).then(|| cx.compound("ref", vec![inner]))
}

fn kernel_relation_goal(cx: &mut Cx, owner: TermId, label: &str, arguments: &[TermId]) -> TermId {
    let label = cx.atom(label);
    let relation = cx.compound("name", vec![owner, label]);
    let list = cx.u.list(arguments);
    let call = cx.compound("call", vec![relation, list]);
    let positive = cx.atom("positive");
    cx.compound("pending_goal", vec![positive, call])
}

/// A body application with bounded parameters builds by rule: `tuple` and `intern`
/// behind the `discharged` goals, one rule per instance edge.
#[allow(clippy::too_many_arguments)]
fn bounded_item(
    cx: &mut Cx,
    owner: TermId,
    node_id: TermId,
    callable: TermId,
    binds: &[EigoBind],
    arguments: &[TermId],
    bounded: &[Bound],
    rule_index: i64,
) -> Result<InstanceRows, TermId> {
    if arguments.iter().any(|argument| cx.u.unary(*argument, "ref").is_none()) {
        let shape = cx.atom("bounded_group_argument");
        let reason = cx.compound("not_built_yet", vec![shape]);
        return Err(cx.diagnostic(node_id, reason));
    }
    let mut out = InstanceRows::default();
    let mut values = arguments.to_vec();
    let mut goals = Vec::new();
    let empty = cx.u.list(&[]);
    for bound in bounded {
        let argument = arguments[bound.column];
        let gate = super::execute::bound_gate(cx, node_id, bound, argument);
        out.rules.push(cx.compound("rule", vec![gate.obligation, empty]));
        goals.push(gate.discharged);
        values[bound.column] = gate.checked;
    }
    let key = cx.compound("generic_arguments", vec![node_id]);
    let tuple = cx.compound("var", vec![key]);
    let key = cx.compound("generic_instance", vec![node_id]);
    let instance = cx.compound("var", vec![key]);
    let mut elements = values.clone();
    elements.push(tuple);
    goals.push(kernel_relation_goal(cx, owner, "tuple", &elements));
    let callable_ref = cx.compound("ref", vec![callable]);
    goals.push(kernel_relation_goal(cx, owner, "intern", &[callable_ref, tuple, instance]));
    let mut next_value = values.iter();
    for (ordinal, bind) in binds.iter().enumerate() {
        let ordinal = cx.int(ordinal as i64);
        let ordinal = cx.compound("const", vec![ordinal]);
        let label = cx.compound("const", vec![bind.label]);
        let mut body = goals.clone();
        let value = if bind.parameter {
            *next_value.next().unwrap_or(&instance)
        } else {
            let key = cx.compound("fixed_bind", vec![node_id, bind.label]);
            let value = cx.compound("var", vec![key]);
            let index = cx.int(bind.index);
            let index = cx.compound("const", vec![index]);
            body.push(kernel_relation_goal(cx, owner, ":", &[callable_ref, label, value, index]));
            value
        };
        let colon = cx.atom(":");
        let relation = cx.compound("name", vec![owner, colon]);
        let list = cx.u.list(&[instance, label, value, ordinal]);
        let head = cx.compound("call", vec![relation, list]);
        let body = cx.u.list(&body);
        out.rules.push(cx.compound("rule", vec![head, body]));
    }
    for (offset, rule) in out.rules.iter().enumerate() {
        let index = rule_index + offset as i64;
        let goals = cx
            .u
            .args::<2>(*rule, "rule")
            .and_then(|[_, body]| cx.u.as_list(body))
            .map_or(0, |body| body.len());
        out.origins
            .extend(super::derived::rule_origins(cx, index, node_id, &vec![node_id; goals]));
    }
    Ok(out)
}
