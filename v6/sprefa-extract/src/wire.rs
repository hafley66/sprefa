//! The FLAT tagged wire: `(family, span, kind, name)` and the edge twin. One
//! flatten, three consumers: the stdout JSONL stream, the store seam adapter, and
//! the parity-golden normalize. `serde` lives on this flat envelope; the generic
//! `Node<F>` never crosses the seam or the stream.
//!
//! The envelope types (`SpanOut`, `FlatFact`) now live in `crate::types`; this
//! module re-exports them and holds the flatten LOGIC. `NodeRef` is flattened to a
//! span at the wire (the local id is meaningless outside one file's node vec), so
//! the JSONL row is span-addressed and self-describing.
//!
//! Epic U: the public surface is `flatten(&ExtractOutput)` + `flatten_jsonl`. The
//! per-family flatteners stay as private helpers; the four per-family `_jsonl`
//! variants are gone (one sorted path serves all).
//!
//! The SCIP projection (`flatten_scip`, `scip_file_edges`) moved to
//! `crate::scip_rows` on size and on subject: this module flattens the four
//! extraction families, that one flattens a foreign tool's index. Both stay
//! re-exported here so no import path moved.

use crate::family::{CallF, CstEdgeKind, CstF, DfF, Family, FlowEdge, FlowF, ProjectEdge, TypeF};
use crate::types::CfgF;
use crate::rows::FamilyBundle;
pub use crate::schema::SCHEMA;
pub use crate::scip_rows::{flatten_scip, scip_file_edges};
use crate::shape::{BlobHash, Strings};
use crate::source::ExtractOutput;
pub use crate::types::{FlatFact, SpanOut};

/// Flatten one file's `ExtractOutput` to flat facts: every present family, in
/// family order (cst, type, call, df). The single flatten the stdout stream, the
/// store seam adapter, and the parity-golden normalize all read. `NodeRef`
/// resolves to a span through each bundle's own node vec; `NameId` resolves to a
/// string through the shared `strings`.
pub fn flatten(out: &ExtractOutput) -> Vec<FlatFact> {
    let mut facts = Vec::new();
    if let Some(bundle) = &out.cst {
        facts.extend(flatten_cst(bundle, &out.strings));
    }
    if let Some(bundle) = &out.types {
        facts.extend(flatten_type(bundle, &out.strings));
    }
    if let Some(bundle) = &out.call {
        facts.extend(flatten_call(bundle, &out.strings));
    }
    if let Some(bundle) = &out.df {
        facts.extend(flatten_df(bundle, &out.strings));
    }
    facts
}

/// Convenience: flatten to sorted JSONL lines. The sort makes the snapshot
/// deterministic across ast-grep/tree-sitter/oxc traversal-order shifts; the store
/// seam and parity normalize use the unsorted `flatten` then their own ordering.
pub fn flatten_jsonl(out: &ExtractOutput) -> Vec<String> {
    let mut lines: Vec<String> = flatten(out)
        .into_iter()
        .map(|fact| serde_json::to_string(&fact).expect("flat fact is serializable"))
        .collect();
    lines.sort();
    lines
}

/// Flatten one CstF bundle to flat facts. `NodeRef` resolves to a span through
/// the bundle's own node vec; `NameId` resolves to a string through `strings`.
fn flatten_cst(bundle: &FamilyBundle<CstF>, strings: &Strings) -> Vec<FlatFact> {
    let mut out = Vec::with_capacity(bundle.nodes.len() + bundle.edges.len());
    for node in &bundle.nodes {
        out.push(FlatFact::Node {
            family: CstF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: strings.lookup(node.kind).to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        });
    }
    for edge in &bundle.edges {
        let from = bundle.node(edge.src);
        let to = bundle.node(edge.dst);
        let kind = match edge.kind {
            CstEdgeKind::Child => "child",
        };
        // A cst edge is a tree child link: the parent/child roles already
        // separate two nodes that share a span, so no endpoint kinds.
        out.push(FlatFact::Edge {
            family: CstF::TAG,
            kind: kind.to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            from_kind: None,
            to: SpanOut::new(to.span.start, to.span.end()),
            to_kind: None,
        });
    }
    out
}

/// Flatten one TypeF bundle to flat facts: entity NODES (kind = the
/// TypeEntityKind slug, name from the interner) + the arrow-type SIGS (each
/// callable's param/return type references) + the CONST value rows. The name-
/// resolved type edges (field / impl / uses / ...) land with `Resolve<TypeF>`.
fn flatten_type(bundle: &FamilyBundle<TypeF>, strings: &Strings) -> Vec<FlatFact> {
    let mut out =
        Vec::with_capacity(bundle.nodes.len() + bundle.aux.sigs.len() + bundle.aux.consts.len() + bundle.aux.docs.len());
    for node in &bundle.nodes {
        out.push(FlatFact::Node {
            family: TypeF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        });
    }
    for sig in &bundle.aux.sigs {
        out.push(FlatFact::Sig {
            family: TypeF::TAG,
            owner: SpanOut::new(sig.owner.start, sig.owner.end()),
            owner_start: sig.owner.start,
            owner_end: sig.owner.end(),
            slot: sig.slot.as_str().to_string(),
            pos: sig.pos,
            ty: strings.lookup(sig.ty).to_string(),
        });
    }
    for c in &bundle.aux.consts {
        out.push(FlatFact::Const {
            family: TypeF::TAG,
            owner: SpanOut::new(c.owner.start, c.owner.end()),
            field: c.field.map(|id| strings.lookup(id).to_string()),
            text: strings.lookup(c.text).to_string(),
            kind: c.kind.as_str().to_string(),
        });
    }
    for doc in &bundle.aux.docs {
        let owner = SpanOut::new(doc.owner.start, doc.owner.end());
        out.push(FlatFact::Doc {
            family: TypeF::TAG,
            owner,
            parent: doc.parent.map(|id| strings.lookup(id).to_string()),
            text: strings.lookup(doc.text).to_string(),
        });
        for tag in &doc.tags {
            out.push(FlatFact::DocTagOut {
                family: TypeF::TAG,
                owner,
                tag: strings.lookup(tag.tag).to_string(),
                arg: tag.arg.map(|id| strings.lookup(id).to_string()),
                text: strings.lookup(tag.text).to_string(),
            });
        }
    }
    out
}

/// Flatten one CallF bundle to flat facts: callable def NODES (kind = the
/// CallKind slug, name from the interner) + call SITE rows (the callee as
/// written, unresolved in phase 1) + module SPECIFIER rows (import/export-from
/// as written; v6-only — no v5 oracle facet). The resolved caller->callee
/// edges land with `Resolve<CallF>`.
fn flatten_call(bundle: &FamilyBundle<CallF>, strings: &Strings) -> Vec<FlatFact> {
    let mut out = Vec::with_capacity(
        bundle.nodes.len()
            + bundle.aux.sites.len()
            + bundle.aux.specifiers.len()
            + bundle.aux.refs.len(),
    );
    for node in &bundle.nodes {
        out.push(FlatFact::Node {
            family: CallF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        });
    }
    for site in &bundle.aux.sites {
        out.push(FlatFact::Site {
            family: CallF::TAG,
            span: SpanOut::new(site.span.start, site.span.end()),
            callee: strings.lookup(site.callee).to_string(),
            callee_path: site.callee_path.map(|id| strings.lookup(id).to_string()),
        });
    }
    for spec in &bundle.aux.specifiers {
        out.push(FlatFact::Specifier {
            family: CallF::TAG,
            span: SpanOut::new(spec.span.start, spec.span.end()),
            name: strings.lookup(spec.name).to_string(),
            kind: spec.kind.as_str().to_string(),
            module: spec.module.map(|id| strings.lookup(id).to_string()),
        });
    }
    for reference in &bundle.aux.refs {
        out.push(FlatFact::Reference {
            family: CallF::TAG,
            span: SpanOut::new(reference.span.start, reference.span.end()),
            functor: strings.lookup(reference.functor).to_string(),
            position: reference.position.as_str().to_string(),
        });
    }
    out
}

/// Flatten one CfgF bundle: control-point NODES + successor EDGES. Entry and
/// Exit share the callable's span, so both endpoint kinds ride every edge.

/// Out of `flatten` on purpose: the CFG is derived from the CstF bundle by
/// `crate::cfg`, never projected by a `Source`, so nothing carries it here.
pub fn flatten_cfg(bundle: &FamilyBundle<CfgF>) -> Vec<FlatFact> {
    let mut out = Vec::with_capacity(bundle.nodes.len() + bundle.edges.len());
    for node in &bundle.nodes {
        out.push(FlatFact::Node {
            family: CfgF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: None,
        });
    }
    for edge in &bundle.edges {
        let from = bundle.node(edge.src);
        let to = bundle.node(edge.dst);
        out.push(FlatFact::Edge {
            family: CfgF::TAG,
            kind: edge.kind.as_str().to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            from_kind: Some(from.kind.as_str().to_string()),
            to: SpanOut::new(to.span.start, to.span.end()),
            to_kind: Some(to.kind.as_str().to_string()),
        });
    }
    out
}

/// Flatten one file's resolved TypeF project edges (phase-2 output) to the ONE
/// project-edge arm. `from` resolves the src `NodeRef` through the PRODUCING
/// file's own TypeF bundle; `to_blob` is the resolved target's content key
/// (hex). Deliberately OUT of `flatten`/`flatten_jsonl` (dispatch stays
/// phase-1 in 4b); the parity golden calls this directly on a `Resolve<TypeF>`
/// result. 4c generalizes to CallF when `Resolve<CallF>` lands.
pub fn flatten_project_type(
    edges: &[ProjectEdge<TypeF>],
    bundle: &FamilyBundle<TypeF>,
) -> Vec<FlatFact> {
    edges
        .iter()
        .map(|edge| {
            let from = bundle.node(edge.src);
            FlatFact::ProjectEdge {
                family: TypeF::TAG,
                kind: edge.kind.as_str().to_string(),
                from: SpanOut::new(from.span.start, from.span.end()),
                to_blob: edge.dst_blob.to_hex(),
                to: SpanOut::new(edge.dst_span.start, edge.dst_span.end()),
            }
        })
        .collect()
}

/// Flatten the FlowF join output to its `flow_edge` arm. Both endpoints are
/// (blob, span) keys; `FlowEdge` already carries flat coordinates.
pub fn flatten_flow(edges: &[FlowEdge]) -> Vec<FlatFact> {
    edges
        .iter()
        .map(|edge| FlatFact::FlowEdgeOut {
            family: FlowF::TAG,
            kind: edge.kind.as_str().to_string(),
            from_blob: edge.src_blob.to_hex(),
            from: SpanOut::new(edge.src_span.start, edge.src_span.end()),
            to_blob: edge.dst_blob.to_hex(),
            to: SpanOut::new(edge.dst_span.start, edge.dst_span.end()),
        })
        .collect()
}

/// One file's identity row: the content key every resolved edge and every
/// phase-2 cache entry is already keyed on, plus its size in bytes and lines.
///
/// The line count is the point. It is v5's `file_lines`, and it was
/// inexpressible from v6 because nothing carried it across the wire even though
/// the extractor holds the bytes. Counting is one pass over content the caller
/// already read, so this costs nothing to produce and saves the consumer from
/// reading every file a second time to count newlines.
///
/// LINE COUNTING CONVENTION: the number of lines a text editor shows. A file
/// with no trailing newline still counts its last partial line, and an empty
/// file has zero lines.
pub fn file_fact(path: &str, content: &[u8]) -> FlatFact {
    let newlines = content.iter().filter(|byte| **byte == b'\n').count();
    let unterminated = !content.is_empty() && !content.ends_with(b"\n");
    FlatFact::FileRow {
        path: path.to_string(),
        digest: BlobHash::of(content).to_hex(),
        bytes: content.len() as u32,
        lines: (newlines + usize::from(unterminated)) as u32,
    }
}

/// Flatten one DfF bundle to flat facts: value-flow NODES (kind = the DfNodeKind
/// slug; name = the variable / property / type when the node carries one) +
/// Direct value EDGES (src value -> dst value). The enclosing callable is
/// derived at the seam (not in the wire). Df argument slots and parameter
/// positions are emitted as flat records; field names and literal texts land
/// in follow-ups.
fn flatten_df(bundle: &FamilyBundle<DfF>, strings: &Strings) -> Vec<FlatFact> {
    let mut out = Vec::with_capacity(
        bundle.nodes.len() + bundle.edges.len() + bundle.aux.params.len() + bundle.aux.args.len(),
    );
    for node in &bundle.nodes {
        out.push(FlatFact::Node {
            family: DfF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        });
    }
    for edge in &bundle.edges {
        let from = bundle.node(edge.src);
        let to = bundle.node(edge.dst);
        out.push(FlatFact::Edge {
            family: DfF::TAG,
            kind: edge.kind.as_str().to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            from_kind: Some(from.kind.as_str().to_string()),
            to: SpanOut::new(to.span.start, to.span.end()),
            to_kind: Some(to.kind.as_str().to_string()),
        });
    }
    for param in &bundle.aux.params {
        let node = bundle.node(param.node);
        out.push(FlatFact::DfParam {
            family: DfF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            pos: param.pos,
        });
    }
    for arg in &bundle.aux.args {
        let call = bundle.node(arg.call);
        let value = bundle.node(arg.arg);
        out.push(FlatFact::DfArg {
            family: DfF::TAG,
            call: SpanOut::new(call.span.start, call.span.end()),
            pos: arg.pos,
            arg: SpanOut::new(value.span.start, value.span.end()),
        });
    }
    out
}
