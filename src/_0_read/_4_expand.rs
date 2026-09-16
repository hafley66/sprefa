//! `expand_dl7/6`. Port of `v7/src/0_reader/1_expander.pl`.
//! Rows grow monotonically in traversal order, so v7's threaded
//! `AvailableRows` list is one insert-if-absent map here.
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashMap;

/// Where v7 `throw`s rather than returning a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// `:148`, `domain_error(dl7_rewrite_tree, _)`.
    RewriteTree,
    /// `:207`, `existence_error(source_row, _)`.
    SourceRow,
}

/// `:29-33`.
pub struct Expanded {
    pub forms: Vec<TermId>,
    pub source_rows: Vec<TermId>,
    pub expansion_rows: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

struct Cx<'a> {
    u: &'a mut Universe,
    /// `memberchk(source(NodeId, ...), Rows)`: the first row for a node wins.
    sources: HashMap<TermId, TermId>,
    rows: Vec<TermId>,
    expansions: Vec<TermId>,
}

impl Cx<'_> {
    fn record(&mut self, row: TermId) {
        if let Some((_, args)) = self.u.functor(row) {
            self.sources.entry(args[0]).or_insert(row);
        }
        self.rows.push(row);
    }
}

/// `:22`.
pub fn expand_dl7(
    u: &mut Universe,
    forms: &[TermId],
    source_rows: &[TermId],
) -> Result<Expanded, Stop> {
    let mut cx = Cx {
        u,
        sources: HashMap::new(),
        rows: Vec::new(),
        expansions: Vec::new(),
    };
    for row in source_rows {
        if let Some(("source", args)) = cx.u.functor(*row) {
            let id = args[0];
            cx.sources.entry(id).or_insert(*row);
        }
    }
    match expand_nodes(&mut cx, forms)? {
        Ok(nodes) => {
            let mut rows = source_rows.to_vec();
            rows.extend(cx.rows);
            Ok(Expanded {
                forms: nodes,
                source_rows: rows,
                expansion_rows: cx.expansions,
                diagnostics: Vec::new(),
            })
        }
        Err(diagnostic) => Ok(Expanded {
            forms: Vec::new(),
            source_rows: Vec::new(),
            expansion_rows: Vec::new(),
            diagnostics: vec![diagnostic],
        }),
    }
}

/// `:46`. The inner `Err` is v7's `error(Diagnostic)`, which stops the walk.
fn expand_nodes(cx: &mut Cx, nodes: &[TermId]) -> Result<Result<Vec<TermId>, TermId>, Stop> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match expand_node(cx, *node)? {
            Ok(expanded) => out.push(expanded),
            Err(diagnostic) => return Ok(Err(diagnostic)),
        }
    }
    Ok(Ok(out))
}

/// `:63`.
fn expand_node(cx: &mut Cx, node: TermId) -> Result<Result<TermId, TermId>, Stop> {
    let node = match expand_node_children(cx, node)? {
        Ok(node) => node,
        Err(diagnostic) => return Ok(Err(diagnostic)),
    };
    let tree = node_tree(cx.u, node);
    rewrite_fixpoint(cx, node, tree, vec![tree], Vec::new(), 1)
}

/// `:75`.
fn expand_node_children(cx: &mut Cx, node: TermId) -> Result<Result<TermId, TermId>, Stop> {
    let Some(("node", args)) = cx.u.functor(node) else {
        return Ok(Ok(node));
    };
    let (node_id, payload) = (args[0], args[1]);
    let Some(children) = cx.u.unary(payload, "form").and_then(|l| cx.u.as_list(l)) else {
        return Ok(Ok(node));
    };
    Ok(match expand_nodes(cx, &children)? {
        Ok(expanded) => {
            let list = cx.u.list(&expanded);
            let form = cx.u.compound("form", vec![list]);
            Ok(cx.u.compound("node", vec![node_id, form]))
        }
        Err(diagnostic) => Err(diagnostic),
    })
}

/// `:92`.
fn rewrite_fixpoint(
    cx: &mut Cx,
    node: TermId,
    tree: TermId,
    mut seen: Vec<TermId>,
    mut trace: Vec<TermId>,
    wave: i64,
) -> Result<Result<TermId, TermId>, Stop> {
    let Some((identity, replacement)) = rewrite(cx.u, tree) else {
        return Ok(Ok(node));
    };
    if !valid_tree(cx.u, replacement) {
        return Err(Stop::RewriteTree);
    }
    // :100. A replacement already on the path is a cycle, reported at the
    // input node with the whole macro path.
    if seen.contains(&replacement) {
        let Some(("node", args)) = cx.u.functor(node) else {
            return Err(Stop::SourceRow);
        };
        let node_id = args[0];
        let mut path = trace;
        path.push(identity);
        let path = cx.u.list(&path);
        let cycle = cx.u.compound("expansion_cycle", vec![path]);
        return Ok(Err(expansion_diagnostic(cx, node_id, cycle)?));
    }
    let Some(("node", args)) = cx.u.functor(node) else {
        return Err(Stop::SourceRow);
    };
    let input_node_id = args[0];
    let minted = mint_tree(cx, replacement, input_node_id, identity, wave, &mut 0)?;
    let minted = match expand_node_children(cx, minted)? {
        Ok(node) => node,
        Err(diagnostic) => return Ok(Err(diagnostic)),
    };
    let next_tree = node_tree(cx.u, minted);
    seen.insert(0, next_tree);
    trace.insert(0, identity);
    rewrite_fixpoint(cx, minted, next_tree, seen, trace, wave + 1)
}

/// `:12-18`, the two multifile clauses, `once/1` so the first match wins.
pub fn rewrite(u: &mut Universe, tree: TermId) -> Option<(TermId, TermId)> {
    let children = u.unary(tree, "form").and_then(|l| u.as_list(l))?;
    let name = u.atom(":");
    let colon = u.compound("atom", vec![name]);
    let identity = u.atom("infix_colon");
    if children.len() >= 2 && children[1] == colon {
        let mut out = vec![colon, children[0]];
        out.extend_from_slice(&children[2..]);
        let list = u.list(&out);
        return Some((identity, u.compound("form", vec![list])));
    }
    let head = *children.first()?;
    let infix = u.unary(head, "atom").and_then(|a| match u.get(a) {
        crate::_6_eval::term::Term::Atom(s) => Some(u.sym_str(*s).to_string()),
        _ => None,
    })?;
    let stem = infix.strip_suffix(':')?;
    if stem.is_empty() {
        return None;
    }
    let stem = u.atom(stem);
    let label = u.compound("atom", vec![stem]);
    let mut out = vec![colon, label];
    out.extend_from_slice(&children[1..]);
    let list = u.list(&out);
    Some((identity, u.compound("form", vec![list])))
}

/// `:151-158`.
fn valid_tree(u: &Universe, tree: TermId) -> bool {
    match u.functor(tree) {
        Some(("atom", args)) if args.len() == 1 => {
            matches!(u.get(args[0]), crate::_6_eval::term::Term::Atom(_))
        }
        Some(("literal", args)) => args.len() == 1,
        Some(("variable", args)) => args.len() == 2,
        Some(("form", args)) if args.len() == 1 => match u.as_list(args[0]) {
            Some(children) => children.iter().all(|c| valid_tree(u, *c)),
            None => false,
        },
        _ => false,
    }
}

/// `:160`. Strips every node id, leaving the payload shape.
pub fn node_tree(u: &mut Universe, node: TermId) -> TermId {
    let Some(("node", args)) = u.functor(node) else {
        return node;
    };
    let payload = args[1];
    let Some(list) = u.unary(payload, "form") else {
        return payload;
    };
    let Some(children) = u.as_list(list) else {
        return payload;
    };
    let trees: Vec<TermId> = children.iter().map(|c| node_tree(u, *c)).collect();
    let list = u.list(&trees);
    u.compound("form", vec![list])
}

/// `:167`. Preorder index, root first.
fn mint_tree(
    cx: &mut Cx,
    tree: TermId,
    input_node_id: TermId,
    identity: TermId,
    wave: i64,
    index: &mut i64,
) -> Result<TermId, Stop> {
    let own = *index;
    *index += 1;
    let wave_term = cx.u.int(wave);
    let own_term = cx.u.int(own);
    let node_id = cx.u.compound(
        "expansion_node",
        vec![input_node_id, identity, wave_term, own_term],
    );
    let source = *cx.sources.get(&input_node_id).ok_or(Stop::SourceRow)?;
    let Some(("source", args)) = cx.u.functor(source) else {
        return Err(Stop::SourceRow);
    };
    let mut fields = vec![node_id];
    fields.extend_from_slice(&args[1..]);
    let generated = cx.u.compound("source", fields);
    cx.record(generated);
    let expansion = cx.u.compound(
        "expansion",
        vec![input_node_id, identity, wave_term, node_id],
    );
    cx.expansions.push(expansion);

    let payload = match cx.u.unary(tree, "form").and_then(|l| cx.u.as_list(l)) {
        Some(children) => {
            let mut nodes = Vec::with_capacity(children.len());
            for child in children {
                nodes.push(mint_tree(cx, child, input_node_id, identity, wave, index)?);
            }
            let list = cx.u.list(&nodes);
            cx.u.compound("form", vec![list])
        }
        None => tree,
    };
    Ok(cx.u.compound("node", vec![node_id, payload]))
}

/// `:215`, five arguments: phase, path, node, code, position.
fn expansion_diagnostic(cx: &mut Cx, node_id: TermId, code: TermId) -> Result<TermId, Stop> {
    let source = *cx.sources.get(&node_id).ok_or(Stop::SourceRow)?;
    let Some(("source", args)) = cx.u.functor(source) else {
        return Err(Stop::SourceRow);
    };
    let (path, start_offset, start_line, start_column) = (args[1], args[2], args[4], args[5]);
    let position =
        cx.u.compound("position", vec![start_offset, start_line, start_column]);
    let phase = cx.u.atom("expansion");
    Ok(cx
        .u
        .compound("diagnostic", vec![phase, path, node_id, code, position]))
}
