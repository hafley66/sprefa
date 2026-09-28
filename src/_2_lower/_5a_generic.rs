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
    let items = forms::form(cx.u, node.payload)?;
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
    let return_atom = cx.atom("return");
    let [return_index] = cx.edges.return_indices(callable, return_atom)[..] else {
        return None;
    };
    let returns_node = cx
        .edges
        .owned(callable)
        .into_iter()
        .find(|edge| edge.index == return_index && edge.name == return_atom)
        .and_then(|edge| cx.u.unary(edge.target, "target"))
        .is_some_and(|inner| cx.u.functor(inner).is_some_and(|(name, _)| name == "owner"));
    if !returns_node || items.len() as i64 != cx.edges.arity(callable) {
        return None;
    }
    let mut values = Vec::with_capacity(items.len() - 1);
    for item in &items[1..] {
        let lowered = lower_expression(cx, *item, owner);
        if !lowered.diagnostics.is_empty() || !lowered.goals.is_empty() {
            return None;
        }
        values.push(lowered.value);
    }
    let callee = cx.compound("name", vec![owner, head]);
    let list = cx.u.list(&values);
    let application = cx.compound("application", vec![callee, list]);
    Some(cx.compound("pending_edge", vec![owner, name, application, index]))
}
