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
use crate::rows::FamilyBundle;
pub use crate::schema::SCHEMA;
pub use crate::scip_rows::{flatten_scip, scip_file_edges};
use crate::shape::{content_id_of, Strings};
use crate::source::ExtractOutput;
use crate::types::{CfgF, DataF};
pub use crate::types::{FlatFact, SpanOut};

/// Flatten one file's `ExtractOutput` to flat facts: every present family, in
/// family order (cst, type, call, df). The single flatten the stdout stream, the
/// store seam adapter, and the parity-golden normalize all read. `NodeRef`
/// resolves to a span through each bundle's own node vec; `NameId` resolves to a
/// string through the shared `strings`.
/// Costs a row vector plus every owned `String` on it; a caller that consumes
/// each row once wants `flatten_each`, which hands the row over and drops it.
pub fn flatten(out: &ExtractOutput) -> Vec<FlatFact> {
    let mut facts = Vec::new();
    let outcome: Result<(), std::convert::Infallible> = flatten_each(out, &mut |fact| {
        facts.push(fact);
        Ok(())
    });
    match outcome {
        Ok(()) => facts,
    }
}

/// One flat fact at a time, in the exact order `flatten` returns them. Fallible
/// because the consumer is a writer: an `io::Error` must stop the walk.
pub fn flatten_each<E>(
    out: &ExtractOutput,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    let span = tracing::debug_span!("flatten", facts = tracing::field::Empty);
    let _entered = span.enter();
    let mut facts = 0u64;
    let push = &mut |fact| {
        facts += 1;
        push(fact)
    };
    let outcome = (|| {
        if let Some(bundle) = &out.cst {
            flatten_cst(bundle, &out.strings, push)?;
        }
        if let Some(bundle) = &out.types {
            flatten_type(bundle, &out.strings, push)?;
        }
        if let Some(bundle) = &out.call {
            flatten_call(bundle, &out.strings, push)?;
        }
        if let Some(bundle) = &out.df {
            flatten_df(bundle, &out.strings, push)?;
        }
        if let Some(bundle) = &out.data {
            flatten_data(bundle, &out.strings, push)?;
        }
        Ok(())
    })();
    span.record("facts", facts);
    outcome
}

/// Object keys sorted, so the wire does not inherit whichever map `serde_json`
/// was built with: `preserve_order` (oxc_resolver's) flips every `data_doc`.
fn key_sorted(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::new();
            for (key, inner) in map {
                sorted.insert(key.clone(), key_sorted(inner));
            }
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(key_sorted).collect())
        }
        other => other.clone(),
    }
}

/// `doc` on a document row is the whole document as a json value, so a consumer
/// declaring only that column reads documents and skips every value row.
fn flatten_data<E>(
    bundle: &FamilyBundle<DataF>,
    strings: &Strings,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    let format = bundle.aux.format;
    for doc in &bundle.aux.docs {
        push(FlatFact::DataDocOut {
            family: DataF::TAG,
            ordinal: doc.ordinal,
            span: SpanOut::new(doc.span.start, doc.span.end()),
            format: format.as_str().to_string(),
            doc: key_sorted(&doc.value),
        })?;
    }
    for row in &bundle.aux.values {
        push(FlatFact::DataValueOut {
            family: DataF::TAG,
            ordinal: row.doc,
            path: strings.lookup(row.path).to_string(),
            kind: row.kind.as_str().to_string(),
            text: row.text.map(|id| strings.lookup(id).to_string()),
            span: SpanOut::new(row.span.start, row.span.end()),
        })?;
    }
    Ok(())
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
fn flatten_cst<E>(
    bundle: &FamilyBundle<CstF>,
    strings: &Strings,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    for node in &bundle.nodes {
        push(FlatFact::Node {
            family: CstF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: strings.lookup(node.kind).to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for edge in &bundle.edges {
        let from = bundle.node(edge.src);
        let to = bundle.node(edge.dst);
        let kind = match edge.kind {
            CstEdgeKind::Child => "child",
        };
        // A cst edge is a tree child link: the parent/child roles already
        // separate two nodes that share a span, so no endpoint kinds.
        push(FlatFact::Edge {
            family: CstF::TAG,
            kind: kind.to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            from_kind: None,
            to: SpanOut::new(to.span.start, to.span.end()),
            to_kind: None,
        })?;
    }
    Ok(())
}

/// Flatten one TypeF bundle to flat facts: entity NODES (kind = the
/// TypeEntityKind slug, name from the interner) + the arrow-type SIGS (each
/// callable's param/return type references) + the CONST value rows. The name-
/// resolved type edges (field / impl / uses / ...) land with `Resolve<TypeF>`.
fn flatten_type<E>(
    bundle: &FamilyBundle<TypeF>,
    strings: &Strings,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    for node in &bundle.nodes {
        push(FlatFact::Node {
            family: TypeF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for sig in &bundle.aux.sigs {
        push(FlatFact::Sig {
            family: TypeF::TAG,
            owner: SpanOut::new(sig.owner.start, sig.owner.end()),
            owner_start: sig.owner.start,
            owner_end: sig.owner.end(),
            slot: sig.slot.as_str().to_string(),
            pos: sig.pos,
            ty: strings.lookup(sig.ty).to_string(),
        })?;
    }
    for c in &bundle.aux.consts {
        push(FlatFact::Const {
            family: TypeF::TAG,
            owner: SpanOut::new(c.owner.start, c.owner.end()),
            field: c.field.map(|id| strings.lookup(id).to_string()),
            text: strings.lookup(c.text).to_string(),
            kind: c.kind.as_str().to_string(),
        })?;
    }
    for doc in &bundle.aux.docs {
        let owner = SpanOut::new(doc.owner.start, doc.owner.end());
        push(FlatFact::Doc {
            family: TypeF::TAG,
            owner,
            parent: doc.parent.map(|id| strings.lookup(id).to_string()),
            text: strings.lookup(doc.text).to_string(),
        })?;
        for tag in &doc.tags {
            push(FlatFact::DocTagOut {
                family: TypeF::TAG,
                owner,
                tag: strings.lookup(tag.tag).to_string(),
                arg: tag.arg.map(|id| strings.lookup(id).to_string()),
                text: strings.lookup(tag.text).to_string(),
            })?;
        }
    }
    for node in &bundle.aux.doc_nodes {
        push(FlatFact::DocNodeOut {
            family: TypeF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: strings.lookup(node.name).to_string(),
            parent: node.parent.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    Ok(())
}

/// Flatten one CallF bundle to flat facts: callable def NODES (kind = the
/// CallKind slug, name from the interner) + call SITE rows (the callee as
/// written, unresolved in phase 1) + module SPECIFIER rows (import/export-from
/// as written; v6-only — no v5 oracle facet). The resolved caller->callee
/// edges land with `Resolve<CallF>`.
fn flatten_call<E>(
    bundle: &FamilyBundle<CallF>,
    strings: &Strings,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    for node in &bundle.nodes {
        push(FlatFact::Node {
            family: CallF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for site in &bundle.aux.sites {
        push(FlatFact::Site {
            family: CallF::TAG,
            span: SpanOut::new(site.span.start, site.span.end()),
            callee: strings.lookup(site.callee).to_string(),
            callee_path: site.callee_path.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for spec in &bundle.aux.specifiers {
        push(FlatFact::Specifier {
            family: CallF::TAG,
            span: SpanOut::new(spec.span.start, spec.span.end()),
            name: strings.lookup(spec.name).to_string(),
            kind: spec.kind.as_str().to_string(),
            module: spec.module.map(|id| strings.lookup(id).to_string()),
            imported: spec.imported.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for owner in &bundle.aux.method_owners {
        push(FlatFact::MethodOwnerOut {
            family: CallF::TAG,
            owner: SpanOut::new(owner.span.start, owner.span.end()),
            self_type: owner.self_type.map(|id| strings.lookup(id).to_string()),
            trait_name: owner.trait_name.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for scope in &bundle.aux.cfg_scopes {
        push(FlatFact::CfgScopeOut {
            family: CallF::TAG,
            span: SpanOut::new(scope.span.start, scope.span.end()),
            cfg: strings.lookup(scope.cfg).to_string(),
        })?;
    }
    for call in &bundle.aux.test_only_calls {
        push(FlatFact::TestOnlyCallOut {
            family: CallF::TAG,
            callee: strings.lookup(call.callee).to_string(),
            cfg: strings.lookup(call.cfg).to_string(),
        })?;
    }
    for reference in &bundle.aux.refs {
        push(FlatFact::Reference {
            family: CallF::TAG,
            span: SpanOut::new(reference.span.start, reference.span.end()),
            functor: strings.lookup(reference.functor).to_string(),
            position: reference.position.as_str().to_string(),
        })?;
    }
    for unresolved in &bundle.aux.unresolved {
        push(FlatFact::Unresolved {
            family: CallF::TAG,
            span: SpanOut::new(unresolved.span.start, unresolved.span.end()),
            reason: unresolved.reason.as_str().to_string(),
            detail: strings.lookup(unresolved.detail).to_string(),
        })?;
    }
    Ok(())
}

/// Flatten one CfgF bundle: control-point NODES + successor EDGES. Entry and
/// Exit share the callable's span, so both endpoint kinds ride every edge.

/// Out of `flatten` on purpose: the CFG is derived from the CstF bundle by
/// `crate::cfg`, never projected by a `Source`, so nothing carries it here.
pub fn flatten_cfg(bundle: &FamilyBundle<CfgF>) -> Vec<FlatFact> {
    let mut facts = Vec::with_capacity(bundle.nodes.len() + bundle.edges.len());
    let outcome: Result<(), std::convert::Infallible> = flatten_cfg_each(bundle, &mut |fact| {
        facts.push(fact);
        Ok(())
    });
    match outcome {
        Ok(()) => facts,
    }
}

/// `flatten_cfg`'s streaming twin, same order, no row vector.
pub fn flatten_cfg_each<E>(
    bundle: &FamilyBundle<CfgF>,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    for node in &bundle.nodes {
        push(FlatFact::Node {
            family: CfgF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: None,
        })?;
    }
    for edge in &bundle.edges {
        let from = bundle.node(edge.src);
        let to = bundle.node(edge.dst);
        push(FlatFact::Edge {
            family: CfgF::TAG,
            kind: edge.kind.as_str().to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            from_kind: Some(from.kind.as_str().to_string()),
            to: SpanOut::new(to.span.start, to.span.end()),
            to_kind: Some(to.kind.as_str().to_string()),
        })?;
    }
    Ok(())
}

/// Flatten one file's resolved TypeF project edges (phase-2 output) to the ONE
/// project-edge arm. `from` resolves the src `NodeRef` through the PRODUCING
/// file's own TypeF bundle; `to_blob` is the resolved target's content key, in
/// `ContentId`'s Display form (`git:`/`blake3:`). Deliberately OUT of `flatten`/`flatten_jsonl` (dispatch stays
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
                to_blob: edge.dst_blob.to_string(),
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
            from_blob: edge.src_blob.to_string(),
            from: SpanOut::new(edge.src_span.start, edge.src_span.end()),
            to_blob: edge.dst_blob.to_string(),
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
        digest: content_id_of(content).to_string(),
        bytes: content.len() as u32,
        lines: (newlines + usize::from(unterminated)) as u32,
    }
}

/// Flatten one DfF bundle to flat facts: value-flow NODES (kind = the DfNodeKind
/// slug; name = the variable / property / type when the node carries one) +
/// Direct value EDGES (src value -> dst value). The enclosing callable is
/// derived at the seam (not in the wire). Df argument slots, parameter
/// positions, field names and literal texts are emitted as flat records.
fn flatten_df<E>(
    bundle: &FamilyBundle<DfF>,
    strings: &Strings,
    push: &mut impl FnMut(FlatFact) -> Result<(), E>,
) -> Result<(), E> {
    for node in &bundle.nodes {
        push(FlatFact::Node {
            family: DfF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            kind: node.kind.as_str().to_string(),
            name: node.name.map(|id| strings.lookup(id).to_string()),
        })?;
    }
    for edge in &bundle.edges {
        let from = bundle.node(edge.src);
        let to = bundle.node(edge.dst);
        push(FlatFact::Edge {
            family: DfF::TAG,
            kind: edge.kind.as_str().to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            from_kind: Some(from.kind.as_str().to_string()),
            to: SpanOut::new(to.span.start, to.span.end()),
            to_kind: Some(to.kind.as_str().to_string()),
        })?;
    }
    for param in &bundle.aux.params {
        let node = bundle.node(param.node);
        push(FlatFact::DfParam {
            family: DfF::TAG,
            span: SpanOut::new(node.span.start, node.span.end()),
            pos: param.pos,
        })?;
    }
    for arg in &bundle.aux.args {
        let call = bundle.node(arg.call);
        let value = bundle.node(arg.arg);
        push(FlatFact::DfArg {
            family: DfF::TAG,
            call: SpanOut::new(call.span.start, call.span.end()),
            pos: arg.pos,
            arg: SpanOut::new(value.span.start, value.span.end()),
        })?;
    }
    for field in &bundle.aux.fields {
        let owner = bundle.node(field.owner);
        let value = bundle.node(field.value);
        push(FlatFact::DfField {
            family: DfF::TAG,
            owner: SpanOut::new(owner.span.start, owner.span.end()),
            name: field.name.clone(),
            value: SpanOut::new(value.span.start, value.span.end()),
        })?;
    }
    for lit in &bundle.aux.lits {
        let node = bundle.node(lit.node);
        push(FlatFact::DfLit {
            family: DfF::TAG,
            node: SpanOut::new(node.span.start, node.span.end()),
            kind: lit.kind.to_string(),
            text: lit.text.clone(),
        })?;
    }
    for loop_row in &bundle.aux.loops {
        push(FlatFact::DfLoop {
            family: DfF::TAG,
            span: SpanOut::new(loop_row.span.start, loop_row.span.end()),
            var: loop_row.var.clone(),
            collection: loop_row.collection.clone(),
        })?;
    }
    for nest in &bundle.aux.nests {
        let call = bundle.node(nest.call);
        push(FlatFact::DfNest {
            family: DfF::TAG,
            call: SpanOut::new(call.span.start, call.span.end()),
            loop_span: SpanOut::new(nest.loop_span.start, nest.loop_span.end()),
            depth: nest.depth,
            collection: nest.collection.clone(),
        })?;
    }
    for allocates in &bundle.aux.allocates {
        push(FlatFact::DfAllocates {
            family: DfF::TAG,
            owner: SpanOut::new(allocates.owner.start, allocates.owner.end()),
        })?;
    }
    Ok(())
}
