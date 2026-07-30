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

use crate::family::{CallF, CstEdgeKind, CstF, DfF, Family, ProjectEdge, TypeF};
use crate::rows::FamilyBundle;
use crate::shape::{BlobHash, Strings};
use crate::source::ExtractOutput;
use crate::types::{OccurrenceRole, ScipIndex};

pub use crate::types::{FlatFact, SpanOut};

/// The JSONL contract, as one block. Keep it in sync with `FlatFact` (the source
/// of truth); this mirrors it for human and AI readers without a doc-build step.
///
/// It lives in the LIBRARY, not the binary, because it describes the library's
/// own wire: the bin prints it under `--schema`, and any other consumer of
/// `flatten` can read the same text without shelling out.
pub const SCHEMA: &str = "\
sprefa-extract JSONL contract: one fact per line, each a JSON object tagged by \
`record`. All spans are half-open byte offsets [start, end) into the file. \
Records join across families by matching spans.

RECORD SHAPES
  record=node   family=<cst|type|call|df>  span={start,end}   kind=<slug>   name=<string|null>
  record=edge   family=<cst|df>            kind=<slug>        from={start,end}  to={start,end}
  record=sig    family=type                owner={start,end}  owner_start=<u32>  owner_end=<u32>  slot=<param|ret>  pos=<u32>  ty=<name>
  record=param  family=df                  span={start,end}   pos=<u32>
  record=arg    family=df                  call={start,end}   pos=<i64>  arg={start,end}
  record=site   family=call                span={start,end}   callee=<name>  callee_path=<string|null>
  record=const  family=type                owner={start,end}  field=<string|null>  text=<string>  kind=<lit|template>
  record=specifier  family=call            span={start,end}   name=<string>  kind=<slug>
  record=capture  query=<id>  capture=<name>  text=<string>  start=<u32>  end=<u32>  match_start=<u32>  match_end=<u32>
  record=resolved_edge  caller_path=<string>  caller_name=<string|null>  callee_path=<string>  callee_name=<string|null>  caller_site_start=<u32>  caller_site_end=<u32>  kind=<slug>
  record=resolved_type_edge  owner_path=<string>  owner_name=<string|null>  owner_start=<u32>  owner_end=<u32>  target_path=<string>  target_name=<string|null>  kind=<slug>
  record=file_edge  src_path=<string>  dst_path=<string>  symbols=<u32>
  record=file  path=<string>  digest=<hex>  bytes=<u32>  lines=<u32>
  record=scip_occurrence  path=<string>  symbol=<string>  start=<u32>  end=<u32>  roles=<i32>  definition=<bool>
  record=scip_symbol  path=<string|null>  symbol=<string>  display_name=<string>  kind=<i32>
  record=scip_relationship  symbol=<string>  related_symbol=<string>  is_reference=<bool>  is_implementation=<bool>  is_type_definition=<bool>  is_definition=<bool>

FIELDS
  family       the graph plane: cst (concrete syntax tree), type (declarations),
               call (callables + call sites), df (intra-procedural value flow).
  span         a node location; half-open bytes.
  kind         the node/edge slug from the per-family vocabulary below.
  name         the declared identifier, when the node carries one (else null).
  owner        the span of the owning declaration (sig/const joins to its callable).
  owner_start  flat start byte of the sig owner span; retained alongside owner for text-host joins.
  owner_end    flat end byte of the sig owner span; retained alongside owner for text-host joins.
  slot         param or ret.
  pos          parameter index (0 for a return slot).
  ty           the referenced type's bare name, UNRESOLVED in phase 1.
  callee       the callee's trailing name as written (the resolution key).
  callee_path  the full qualified path when >1 segment (filled by resolution; else null).
  field        dotted path into an object const, or an enum member (else null).
  text         the resolved string value of a const.
  query        caller-supplied identity for one batched ast-grep pattern.
  capture      one requested single-node ast-grep metavariable.
  start/end    capture's half-open byte span in pattern mode.
  match_start/match_end  whole pattern match's half-open byte span.
  caller_site_start  start byte of the call site that produced a resolved edge.
  caller_site_end    end byte of the call site that produced a resolved edge.
  owner_path   file holding the declaration that makes a resolved type reference.
  target_path  file holding the declaration a resolved type reference names.
  digest       the file's content key, the same one resolved edges are keyed on.
  bytes        the file's length in bytes.
  lines        the file's line count as an editor shows it: an unterminated last
               line still counts, an empty file is 0.
  symbol       a SCIP symbol string; `local `-prefixed symbols are document-scoped.
  roles        the raw scip.proto SymbolRole bitfield, kept whole.
  definition   roles & DEFINITION, hoisted out of the bitfield.
  display_name the symbol's name as scip records it.
  related_symbol  the other end of a scip.proto Relationship.
  src_path/dst_path  the two ends of a file dependency edge.
  symbols      how many distinct symbols cross one file edge.

KIND VOCABULARIES (the `kind` field)
  type node   struct enum trait class interface alias function method const
  call node   function method lambda
  df node     param let_bind var_read var_write lit call_res new member ret
              borrow binop unop loop if match block closure try break expr
              cond logic concat template
  cst node    the grammar node type as named by ast-grep / tree-sitter (open set)
  cst edge    child
  df edge     direct
  const kind  lit (cooked literal) | template (raw source slice, holes intact)
  sig slot    param | ret
  resolved_edge kind       name_resolve | scip_override
  resolved_type_edge kind  field | impl | variant | generic | uses

PHASE-1 LIMITS (default mode)
  No name resolution: type edges, caller->callee links, and cross-file joins are
  NOT emitted. `site` records carry the callee name as written; `sig` records
  carry the referenced type's bare name.

SCIP FACTS MODE (--scip-facts)
  Streams a loaded SCIP index as raw rows: scip_occurrence, scip_symbol,
  scip_relationship. Deliberately UNJOINED. A definition is an occurrence with
  definition true, a reference is one without, a local is a `local `-prefixed
  symbol, and an implements edge is a scip_relationship with is_implementation.
  Those filters and joins belong above this binary.

DEPENDENCY EDGES (--scip-deps)
  Folds a SCIP index into file_edge rows: v6's module graph, produced with no
  module resolver in the crate at all. Graded against madge over 212 real
  TypeScript files at recall 0.992 and precision 0.988.

FILE FACT (--file-fact)
  Prepends one `file` row to the normal stream, carrying the content digest,
  byte count and line count. It rides the same read as extraction.

PROJECT MODE (--resolve)
  `--resolve PATH...` runs phase 2 over the supplied files as one project.
  `--family call` (the default) emits `resolved_edge`; `--family type` emits
  `resolved_type_edge`; `--family call,type` emits both. Adding
  `--project-root DIR` with `--scip-index FILE` or `--scip-build` puts a SCIP
  index in the resolve context, which lets the call arm emit `scip_override`
  rows where the indexer disagrees with the name match.";

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
        out.push(FlatFact::Edge {
            family: CstF::TAG,
            kind: kind.to_string(),
            from: SpanOut::new(from.span.start, from.span.end()),
            to: SpanOut::new(to.span.start, to.span.end()),
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
        Vec::with_capacity(bundle.nodes.len() + bundle.aux.sigs.len() + bundle.aux.consts.len());
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
    out
}

/// Flatten one CallF bundle to flat facts: callable def NODES (kind = the
/// CallKind slug, name from the interner) + call SITE rows (the callee as
/// written, unresolved in phase 1) + module SPECIFIER rows (import/export-from
/// as written; v6-only — no v5 oracle facet). The resolved caller->callee
/// edges land with `Resolve<CallF>`.
fn flatten_call(bundle: &FamilyBundle<CallF>, strings: &Strings) -> Vec<FlatFact> {
    let mut out = Vec::with_capacity(
        bundle.nodes.len() + bundle.aux.sites.len() + bundle.aux.specifiers.len(),
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

/// Flatten a loaded SCIP index to raw flat facts: every occurrence, every symbol
/// information row, every relationship.
///
/// DELIBERATELY UNJOINED. v5 spells ten separate relations over this data
/// (`scip_occurrence`, `scip_def`, `scip_ref`, `scip_name`, `scip_local`,
/// `scip_binding`, `scip_impl`, `scip_edge`, `scip_fn_edge`,
/// `scip_callee_type`). Every one of them is a filter or a join over these three
/// rows, and the standing law puts those machines in the dl layer: the extractor
/// stays a fact producer. Projecting ten relations here would also weld v5's
/// relation vocabulary into a crate that must survive the rust port unchanged.
///
/// `reader` supplies each document's bytes, because SCIP ranges are (line, col)
/// in the document's own position encoding and this wire is byte offsets. A
/// document the reader cannot read contributes its symbols and relationships but
/// no occurrences, and an occurrence whose range does not convert is dropped
/// rather than clamped into a lie (the `byte_range` law).
pub fn flatten_scip(index: &ScipIndex, reader: &dyn Fn(&str) -> Option<Vec<u8>>) -> Vec<FlatFact> {
    let mut out = Vec::new();
    for document in &index.documents {
        let content = reader(&document.relative_path);
        if let Some(content) = &content {
            for occurrence in &document.occurrences {
                let Some(span) =
                    crate::scip::byte_range(content, occurrence.range, document.position_encoding)
                else {
                    continue;
                };
                out.push(FlatFact::ScipOccurrenceRow {
                    path: document.relative_path.clone(),
                    symbol: occurrence.symbol.clone(),
                    start: span.start,
                    end: span.end(),
                    roles: occurrence.roles.0,
                    definition: occurrence.roles.contains(OccurrenceRole::DEFINITION),
                });
            }
        }
        for info in &document.symbols {
            out.push(FlatFact::ScipSymbolRow {
                path: Some(document.relative_path.clone()),
                symbol: info.symbol.clone(),
                display_name: info.display_name.clone(),
                kind: info.kind,
            });
            out.extend(relationship_rows(info));
        }
    }
    // External symbols belong to no document: they are what the corpus
    // references and does not define, which is exactly how a consumer tells a
    // library call from a corpus call.
    for info in &index.external_symbols {
        out.push(FlatFact::ScipSymbolRow {
            path: None,
            symbol: info.symbol.clone(),
            display_name: info.display_name.clone(),
            kind: info.kind,
        });
        out.extend(relationship_rows(info));
    }
    out
}

fn relationship_rows(info: &crate::types::ScipSymbolInfo) -> Vec<FlatFact> {
    info.relationships
        .iter()
        .map(|related| FlatFact::ScipRelationshipRow {
            symbol: info.symbol.clone(),
            related_symbol: related.symbol.clone(),
            is_reference: related.is_reference,
            is_implementation: related.is_implementation,
            is_type_definition: related.is_type_definition,
            is_definition: related.is_definition,
        })
        .collect()
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
            to: SpanOut::new(to.span.start, to.span.end()),
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

/// Fold a loaded SCIP index into file-to-file dependency edges: `src` holds a
/// non-definition occurrence of a symbol defined in `dst`.
///
/// WHY THIS IS THE ONE DERIVED RELATION THE EXTRACTOR PROJECTS. The fold is
/// exactly the join a dl rule would write over `flatten_scip`'s occurrence rows,
/// and by the standing law those machines live in prolog. It is here anyway
/// because the amplification is measured: over v6/tsv2, 212 TypeScript files
/// produce 122,317 occurrence rows and 755 edges. Sending 122,317 rows across
/// the wire to compute 755 is the shape the N+1 law exists to stop. The raw rows
/// remain available for every join that is not this one.
///
/// GRADED AGAINST MADGE over that same corpus: 746 of madge's 752 edges agree,
/// recall 0.992, precision 0.988. Both divergence classes are understood and
/// neither is a resolution error.
///  - 6 madge-only edges are all from `labs/`, which the corpus tsconfig's
///    `include` omits. scip-typescript indexes the tsconfig program; madge walks
///    the filesystem. The two tools disagree about the corpus, not the graph.
///  - 9 scip-only edges are all references to declarations in one types module
///    from files with NO import of it. SCIP sees an inferred type reaching them
///    through another module's signature; madge scans import syntax and cannot.
///    SCIP is right and strictly sees more, the same shape as the flagship
///    callgraph result.
///
/// LOCAL SYMBOLS ARE EXCLUDED. scip reuses `local N` per document, so a `local`
/// symbol in two files is two different things and joining on it would mint
/// edges between unrelated files.
pub fn scip_file_edges(index: &ScipIndex) -> Vec<FlatFact> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut defining_document: BTreeMap<&str, &str> = BTreeMap::new();
    for document in &index.documents {
        for occurrence in &document.occurrences {
            if !occurrence.roles.contains(OccurrenceRole::DEFINITION)
                || occurrence.symbol.starts_with("local ")
            {
                continue;
            }
            defining_document
                .entry(&occurrence.symbol)
                .or_insert(&document.relative_path);
        }
    }

    // (src, dst) -> the distinct symbols crossing it. A BTreeSet both dedups a
    // symbol referenced many times in one file and keeps the output ordered.
    let mut crossings: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
    for document in &index.documents {
        for occurrence in &document.occurrences {
            if occurrence.roles.contains(OccurrenceRole::DEFINITION)
                || occurrence.symbol.starts_with("local ")
            {
                continue;
            }
            let Some(target) = defining_document.get(occurrence.symbol.as_str()) else {
                continue;
            };
            if *target == document.relative_path {
                continue;
            }
            crossings
                .entry((&document.relative_path, target))
                .or_default()
                .insert(&occurrence.symbol);
        }
    }

    crossings
        .into_iter()
        .map(|((src, dst), symbols)| FlatFact::FileEdgeRow {
            src_path: src.to_string(),
            dst_path: dst.to_string(),
            symbols: symbols.len() as u32,
        })
        .collect()
}
