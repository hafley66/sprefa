use super::cx::Cx;
use crate::_6_eval::term::TermId;
use super::execute::{term_has_var, goal_mentions};

/// A tuple built in a rule body and handed to a relation call is output of the
/// rule: one sibling rule per position writes `(: Tuple k Element k)` from the
/// goals up to the tuple goal, so a position reads the same for a written
/// tuple and a built one. A tuple whose elements are not all bound by earlier
/// goals is the callee's own construction and writes no positions.
pub fn tuple_position_rules(
    cx: &mut Cx,
    owner: TermId,
    goals: &[TermId],
    goal_nodes: &[TermId],
) -> Vec<(TermId, Vec<TermId>)> {
    let mut out = Vec::new();
    for (at, goal) in goals.iter().enumerate() {
        let Some(("pending_goal", parts)) = cx.u.functor(*goal) else {
            continue;
        };
        let Some(("call", call)) = cx.u.functor(parts[1]) else {
            continue;
        };
        let Some(("name", name)) = cx.u.functor(call[0]) else {
            continue;
        };
        if cx.u.functor_or_atom(name[1]).map(|(n, _)| n) != Some("tuple") {
            continue;
        }
        let Some(arguments) = cx.u.as_list(call[1]) else {
            continue;
        };
        let Some((whole, elements)) = arguments.split_last() else {
            continue;
        };
        let earlier: Vec<TermId> = goals[..at].to_vec();
        let bound = |cx: &Cx, term: TermId| {
            !term_has_var(cx, term) || earlier.iter().any(|g| goal_mentions(cx, *g, term))
        };
        if !elements.iter().all(|e| bound(cx, *e)) {
            continue;
        }
        let handed_to_relation = goals[at + 1..].iter().any(|later| {
            let Some(("pending_goal", later_parts)) = cx.u.functor(*later) else {
                return false;
            };
            let Some(("call", later_call)) = cx.u.functor(later_parts[1]) else {
                return false;
            };
            let Some(("name", later_name)) = cx.u.functor(later_call[0]) else {
                return false;
            };
            let label = cx
                .u
                .functor_or_atom(later_name[1])
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
            super::kernel::kernel_relation(None, &label).is_none()
                && !super::kernel::kernel_variadic(None, &label)
                && goal_mentions(cx, later_parts[1], *whole)
        });
        let handed_on = handed_to_relation || handed_to_generic(cx, &goals[at + 1..], *whole);
        if !handed_on {
            continue;
        }
        for (position, element) in elements.iter().enumerate() {
            let index = cx.int(position as i64);
            let index = cx.compound("const", vec![index]);
            let colon = cx.atom(":");
            let relation = cx.compound("name", vec![owner, colon]);
            let list = cx.u.list(&[*whole, index, *element, index]);
            let head = cx.compound("call", vec![relation, list]);
            let body = cx.u.list(&goals[..=at]);
            let rule = cx.compound("rule", vec![head, body]);
            out.push((rule, goal_nodes[..=at.min(goal_nodes.len() - 1)].to_vec()));
        }
    }
    out
}

/// A tuple that is a column of a generic application: an element of the
/// argument tuple a later `intern` or `intern_snapshot` of a generic callee
/// reads.
fn handed_to_generic(cx: &mut Cx, later: &[TermId], whole: TermId) -> bool {
    let calls: Vec<(String, Vec<TermId>)> = later
        .iter()
        .filter_map(|goal| {
            let ("pending_goal", parts) = cx.u.functor(*goal)? else {
                return None;
            };
            let ("call", call) = cx.u.functor(parts[1])? else {
                return None;
            };
            let [_, label] = cx.u.args::<2>(call[0], "name")?;
            let label = cx.u.functor_or_atom(label)?.0.to_string();
            Some((label, cx.u.as_list(call[1])?))
        })
        .collect();
    calls.iter().any(|(label, arguments)| {
        let Some((argument_tuple, columns)) = arguments.split_last() else {
            return false;
        };
        label == "tuple"
            && columns.contains(&whole)
            && calls.iter().any(|(reader, read)| {
                matches!(reader.as_str(), "intern" | "intern_snapshot")
                    && read.len() == 3
                    && read[1] == *argument_tuple
                    && cx
                        .u
                        .unary(read[0], "ref")
                        .is_some_and(|callee| super::generic::is_generic_callable(cx, callee))
            })
    })
}
