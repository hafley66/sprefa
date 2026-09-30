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
    if !is_generic_callable(cx, callable) {
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
