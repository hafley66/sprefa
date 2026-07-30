//! The JSONL output contract as one block of text.
//!
//! Split out of `wire.rs` on size: that module is the flatten LOGIC and this is
//! documentation, so they change for different reasons and one of them is 100
//! lines of prose. `wire` re-exports `SCHEMA` so no import path moved.
//!
//! It lives in the LIBRARY, not the binary, because it describes the library's
//! own wire: the bin prints it under `--schema`, and any other consumer of
//! `flatten` reads the same contract without shelling out.

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
