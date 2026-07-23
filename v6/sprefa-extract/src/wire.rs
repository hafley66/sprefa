//! The FLAT tagged wire: `(family, span, kind, name)` and the edge twin. One
//! flatten, three consumers: the stdout JSONL stream, the store seam adapter,
//! and the parity-golden normalize. `serde` lives on this flat envelope; the
//! generic `Node<F>` never crosses the seam or the stream.
//!
//! `NodeRef` is flattened to a span at the wire (the local id is meaningless
//! outside one file's node vec), so the JSONL row is span-addressed and
//! self-describing.

use serde::Serialize;

use crate::family::{CstEdgeKind, CstF, Family};
use crate::rows::FamilyBundle;
use crate::shape::{FamilyTag, Strings};

/// A span on the wire: inclusive-exclusive byte offsets into the file.
#[derive(Copy, Clone, Serialize, Debug)]
pub struct SpanOut {
    pub start: u32,
    pub end: u32,
}

impl SpanOut {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// One flat fact. The `record` tag discriminates node vs edge; `family` carries
/// the family plane. Serialized as JSONL (`{"record":"node",...}` /
/// `{"record":"edge",...}`).
#[derive(Serialize, Debug)]
#[serde(tag = "record", rename_all = "lowercase")]
pub enum FlatFact {
    Node {
        family: FamilyTag,
        span: SpanOut,
        kind: String,
        name: Option<String>,
    },
    Edge {
        family: FamilyTag,
        kind: String,
        from: SpanOut,
        to: SpanOut,
    },
}

/// Flatten one CstF bundle (+ the strings it interned into) to flat facts.
/// `NodeRef` resolves to a span through the bundle's own node vec; `NameId`
/// resolves to a string through `strings`.
pub fn flatten_cst(bundle: &FamilyBundle<CstF>, strings: &Strings) -> Vec<FlatFact> {
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
        out.push(FlatFact::Edge {
            family: CstF::TAG,
            kind: kind.to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            to: SpanOut::new(to.span.start, to.span.end()),
        });
    }
    out
}

/// Convenience: flatten to sorted JSONL lines. The sort makes the snapshot
/// deterministic across ast-grep/tree-sitter traversal-order shifts; the store
/// seam and parity normalize use the same flatten then their own ordering.
pub fn flatten_cst_jsonl(bundle: &FamilyBundle<CstF>, strings: &Strings) -> Vec<String> {
    let mut lines: Vec<String> = flatten_cst(bundle, strings)
        .into_iter()
        .map(|fact| serde_json::to_string(&fact).expect("flat fact is serializable"))
        .collect();
    lines.sort();
    lines
}
