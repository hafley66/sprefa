//! The instance a bind builds from `application(Callee, Args)`: the node is the
//! `application(Template, Args)` term `intern_row` mints, and every edge of the
//! template's `return` expression is a ground `:` row over it.

use super::graph::CheckerGraph;
use super::resolve::{resolve_argument, resolve_target, Cx};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashSet;

/// Nesting of `(* ...)` and `(+ ...)` inside one `return` expression.
const INSTANCE_DEPTH: usize = 16;

struct Build<'a> {
    graph: &'a CheckerGraph,
    columns: Vec<(TermId, TermId)>,
    arguments: TermId,
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
) -> Result<Vec<TermId>, TermId> {
    let Some([callee, list]) = u.args::<2>(application, "application") else {
        return Err(u.atom("invalid_application"));
    };
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
        resolved.push(resolve_argument(u, cx, *item)?);
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
    };
    let root = u.compound("application", vec![template, arguments]);
    let root_ref = u.compound("ref", vec![root]);
    let index_term = u.int(index);
    let mut rows = vec![u.compound(":", vec![owner, name, root_ref, index_term])];
    if built.insert(root) {
        walk(u, cx, &build, template, root, 0, &mut rows)?;
    }
    Ok(rows)
}

fn walk(
    u: &mut Universe,
    cx: &Cx,
    build: &Build,
    template: TermId,
    instance: TermId,
    depth: usize,
    rows: &mut Vec<TermId>,
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
            walk(u, cx, build, inner, child, depth + 1, rows)?;
            u.compound("ref", vec![child])
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
