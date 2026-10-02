//! The instance a bind builds from `application(Callee, Args)`: the node is the
//! `application(Callee, Args)` term `intern_row` mints, the same id a rule
//! builds with `intern`, and every edge of the callee's `return` expression is
//! a ground `:` row over it. The instance's `intern` and `intern_snapshot` rows
//! are seeds, so a readback sees a bound instance and a built one alike.

use super::graph::CheckerGraph;
use super::resolve::{intern_calls, resolve_argument, resolve_target, Cx};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashSet;

/// Nesting of `(* ...)` and `(+ ...)` inside one `return` expression.
const INSTANCE_DEPTH: usize = 16;

struct Build<'a> {
    graph: &'a CheckerGraph,
    columns: Vec<(TermId, TermId)>,
    arguments: TermId,
    tagged: Vec<TermId>,
}

/// The rows of one bind, or the reason it did not resolve.
pub fn instance_rows(
    u: &mut Universe,
    cx: &Cx,
    owner: TermId,
    name: TermId,
    index: i64,
    application: TermId,
    built: &mut HashSet<TermId>,
    minted: &mut Vec<TermId>,
) -> Result<Vec<TermId>, TermId> {
    let mut rows = Vec::new();
    let root_ref = instance(u, cx, name, application, built, 0, &mut rows, minted)?;
    let index_term = u.int(index);
    rows.insert(0, u.compound(":", vec![owner, name, root_ref, index_term]));
    Ok(rows)
}

/// Nesting of generic applications inside generic arguments.
const ARGUMENT_DEPTH: usize = 16;

/// Builds the instance of one application, inner argument applications first.
/// Returns `ref(application(Callee, Args))`; the edges go to `rows`, the
/// `intern` rows to `minted`.
#[allow(clippy::too_many_arguments)]
fn instance(
    u: &mut Universe,
    cx: &Cx,
    name: TermId,
    application: TermId,
    built: &mut HashSet<TermId>,
    depth: usize,
    rows: &mut Vec<TermId>,
    minted: &mut Vec<TermId>,
) -> Result<TermId, TermId> {
    if depth >= ARGUMENT_DEPTH {
        return Err(u.atom("instance_depth_exceeded"));
    }
    let Some([callee, list]) = u.args::<2>(application, "application") else {
        return Err(u.atom("invalid_application"));
    };
    let tuple = u.atom("tuple");
    if u.get(callee) == u.get(tuple) {
        return tuple_instance(u, cx, application, list, 0, rows);
    }
    let mut visited = Vec::new();
    let callee = resolve_target(u, cx, callee, &mut visited)
        .and_then(|resolved| u.unary(resolved, "ref"))
        .ok_or_else(|| u.compound("unresolved_name", vec![name]))?;
    let return_atom = u.atom("return");
    let template = cx
        .graph
        .forward(callee, return_atom)
        .and_then(|target| u.unary(target, "target"))
        .ok_or_else(|| u.compound("expression_without_return", vec![callee]))?;
    let mut columns = Vec::new();
    let mut seen = HashSet::new();
    for position in cx.graph.by_owner.get(&callee).cloned().unwrap_or_default() {
        let edge = &cx.graph.edges[position];
        if edge.name != return_atom && seen.insert(edge.index) {
            columns.push((edge.index, edge.name));
        }
    }
    columns.sort_by_key(|(index, _)| *index);
    let items = u.as_list(list).unwrap_or_default();
    if items.len() != columns.len() {
        let expected = u.int(columns.len() as i64);
        let observed = u.int(items.len() as i64);
        return Err(u.compound("expression_arity_mismatch", vec![name, expected, observed]));
    }
    let mut resolved = Vec::with_capacity(items.len());
    for item in &items {
        if u.args::<2>(*item, "application").is_some() {
            resolved.push(instance(u, cx, name, *item, built, depth + 1, rows, minted)?);
        } else {
            resolved.push(resolve_argument(u, cx, *item)?);
        }
    }
    let plain: Vec<TermId> = resolved
        .iter()
        .map(|tagged| {
            u.unary(*tagged, "ref")
                .or_else(|| u.unary(*tagged, "const"))
                .unwrap_or(*tagged)
        })
        .collect();
    let arguments = u.list(&plain);
    let build = Build {
        graph: cx.graph,
        columns: columns
            .iter()
            .zip(&resolved)
            .map(|((_, label), value)| (*label, *value))
            .collect(),
        arguments,
        tagged: resolved.clone(),
    };
    let root = u.compound("application", vec![callee, arguments]);
    let root_ref = u.compound("ref", vec![root]);
    if built.insert(root) {
        intern_calls(u, callee, &resolved, root_ref, minted);
        walk(u, cx, &build, template, root, 0, rows, minted)?;
    }
    Ok(root_ref)
}

/// Nesting of `[..]` literals inside one tuple.
const TUPLE_DEPTH: usize = 16;

/// `application(tuple, Items)`: one `:` row per item, label and index both
/// the position. Nested tuple applications mint their own rows.
fn tuple_instance(
    u: &mut Universe,
    cx: &Cx,
    application: TermId,
    list: TermId,
    depth: usize,
    rows: &mut Vec<TermId>,
) -> Result<TermId, TermId> {
    let depth_exceeded = depth >= TUPLE_DEPTH;
    if depth_exceeded {
        return Err(u.atom("tuple_depth_exceeded"));
    }
    let tuple = u.atom("tuple");
    let items = u.as_list(list).unwrap_or_default();
    for (position, item) in items.iter().enumerate() {
        if let Some([callee, inner]) = u.args::<2>(*item, "application") {
            if u.get(callee) == u.get(tuple) {
                tuple_instance(u, cx, *item, inner, depth + 1, rows)?;
            }
        }
        let value = resolve_argument(u, cx, *item)?;
        let ordinal = u.int(position as i64);
        rows.push(u.compound(":", vec![application, ordinal, value, ordinal]));
    }
    Ok(u.compound("ref", vec![application]))
}

/// The `:` rows of `instance` from `template`; a nested expression is the node
/// `application(Expression, Args)` with its own `intern` rows, as a rule
/// builds it.
#[allow(clippy::too_many_arguments)]
fn walk(
    u: &mut Universe,
    cx: &Cx,
    build: &Build,
    template: TermId,
    instance: TermId,
    depth: usize,
    rows: &mut Vec<TermId>,
    minted: &mut Vec<TermId>,
) -> Result<(), TermId> {
    let depth_exceeded = depth >= INSTANCE_DEPTH;
    if depth_exceeded {
        return Err(u.atom("instance_depth_exceeded"));
    }
    let mut seen = HashSet::new();
    for position in build.graph.by_owner.get(&template).cloned().unwrap_or_default() {
        let edge = build.graph.edges[position].clone();
        if !seen.insert(edge.index) {
            continue;
        }
        let nested = u.unary(edge.target, "target");
        let column = u
            .args::<2>(edge.target, "name")
            .and_then(|[_, label]| build.columns.iter().find(|(name, _)| *name == label));
        let value = if let Some(inner) = nested {
            let child = u.compound("application", vec![inner, build.arguments]);
            let child_ref = u.compound("ref", vec![child]);
            intern_calls(u, inner, &build.tagged, child_ref, minted);
            walk(u, cx, build, inner, child, depth + 1, rows, minted)?;
            child_ref
        } else if let Some((_, value)) = column {
            *value
        } else {
            let mut visited = Vec::new();
            resolve_target(u, cx, edge.target, &mut visited)
                .ok_or_else(|| u.compound("unresolved_name", vec![edge.name]))?
        };
        let index = u.int(edge.index);
        rows.push(u.compound(":", vec![instance, edge.name, value, index]));
    }
    Ok(())
}
