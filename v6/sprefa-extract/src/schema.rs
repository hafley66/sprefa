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
  record=reference  family=call            span={start,end}   functor=<name/arity>  position=<goal|head_arg|term_arg>
  record=const  family=type                owner={start,end}  field=<string|null>  text=<string>  kind=<lit|template>
  record=specifier  family=call            span={start,end}   name=<string>  kind=<slug>  module=<string|null>
  record=capture  query=<id>  capture=<name>  text=<string>  start=<u32>  end=<u32>  match_start=<u32>  match_end=<u32>
  record=resolved_edge  caller_path=<string>  caller_name=<string|null>  callee_path=<string>  callee_name=<string|null>  caller_site_start=<u32>  caller_site_end=<u32>  kind=<slug>
  record=resolved_type_edge  owner_path=<string>  owner_name=<string|null>  owner_start=<u32>  owner_end=<u32>  target_path=<string>  target_name=<string|null>  kind=<slug>
  record=file_edge  src_path=<string>  dst_path=<string>  symbols=<u32>
  record=file  path=<string>  digest=<hex>  bytes=<u32>  lines=<u32>
  record=scip_metadata  version=<i32>  tool_name=<string>  tool_version=<string>  tool_arguments=[<string>]  project_root=<string>  text_document_encoding=<i32>
  record=scip_document  path=<string>  language=<string>  position_encoding=<i32>  text=<string|null>
  record=scip_occurrence  path=<string>  symbol=<string>  start=<u32>  end=<u32>  roles=<i32>  definition=<bool>  import=<bool>  write_access=<bool>  read_access=<bool>  generated=<bool>  test=<bool>  forward_definition=<bool>  syntax_kind=<i32>  enclosing_start=<u32|null>  enclosing_end=<u32|null>
  record=scip_occurrence_doc  path=<string>  start=<u32>  end=<u32>  pos=<u32>  text=<string>
  record=scip_diagnostic  path=<string>  start=<u32>  end=<u32>  severity=<i32>  code=<string>  message=<string>  source=<string>  tags=[<i32>]
  record=scip_symbol  path=<string|null>  symbol=<string>  display_name=<string>  kind=<i32>  enclosing_symbol=<string>
  record=scip_relationship  symbol=<string>  related_symbol=<string>  is_reference=<bool>  is_implementation=<bool>  is_type_definition=<bool>  is_definition=<bool>
  record=scip_documentation  symbol=<string>  pos=<u32>  text=<string>
  record=scip_signature  symbol=<string>  language=<string>  text=<string>
  record=scip_signature_occurrence  symbol=<string>  ref_symbol=<string>  start=<u32>  end=<u32>  roles=<i32>
  record=scip_index  reused=<bool>  tool_name=<string>  tool_version=<string>  documents=<u32>
  record=scip_skip  lang=<string>  bin=<string>  reason=<not_installed|timed_out|failed>  detail=<string>
  record=scip_def  symbol=<string>  file=<string>  repo=<string>
  record=scip_name  symbol=<string>  name=<string>
  record=scip_ref  file=<string>  symbol=<string>  def_file=<string>  repo=<string>
  record=scip_edge  src=<string>  dst=<string>  repo=<string>
  record=scip_fn_edge  caller=<string>  callee=<string>
  record=scip_callee_type  sym=<string>  type=<string>
  record=scip_local  fn=<string>  name=<string>
  record=scip_impl  impl=<string>  iface=<string>

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
  functor      a Prolog term-occurrence: the interned `name/arity` key at a span.
  position     where a Prolog term-occurrence sits: goal (executed as a body
               goal), head_arg (inside a clause head's arguments), term_arg
               (inside another term's arguments, data).
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
  module       a specifier's source module as written, null when the language
               puts the module in `name` (path-only forms).
  symbol       a SCIP symbol string; `local `-prefixed symbols are document-scoped.
  roles        the raw scip.proto SymbolRole bitfield, kept whole.
  definition / import / write_access / read_access / generated / test /
  forward_definition  the seven SymbolRole bits, one column each.
  syntax_kind  raw scip.proto SyntaxKind ordinal (0 = unspecified).
  enclosing_start/enclosing_end  the nearest enclosing AST node's byte span,
               null when the indexer emitted none.
  display_name the symbol's name as scip records it.
  enclosing_symbol  the owner of a local symbol; empty for global symbols.
  related_symbol  the other end of a scip.proto Relationship.
  ref_symbol   a symbol referenced inside a signature's text; the start/end on
               that record are offsets into the SIGNATURE TEXT, not a document.
  severity/tags  raw scip.proto Severity and DiagnosticTag ordinals.
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
  Streams a loaded SCIP index as raw rows, EVERY field the protobuf serializes:
  scip_metadata, scip_document, scip_occurrence, scip_occurrence_doc,
  scip_diagnostic, scip_symbol, scip_relationship, scip_documentation,
  scip_signature, scip_signature_occurrence. Deliberately UNJOINED. A definition
  is an occurrence with definition true, a reference is one without, a local is
  a `local `-prefixed symbol, and an implements edge is a scip_relationship with
  is_implementation. Those filters and joins belong above this binary.

  The one thing not passed through is the scip.proto Symbol / Package /
  Descriptor message family, which is never serialized into an index: those
  messages describe the grammar of the symbol STRING, which is emitted verbatim.

  --scip-record KINDS narrows the stream. Full passthrough over v6/tsv2 (204
  indexed documents) is 177,967 rows and 59.4MB, of which scip_occurrence alone
  is 123,655 rows and 48.5MB.

DEPENDENCY EDGES (--scip-deps and --deps)
  Both fold to the SAME file_edge record, so the module graph is one relation
  regardless of which resolver filled it.
  --scip-deps folds a SCIP index: the indexer already resolved every reference,
  so the graph falls out of the index with no module resolver in the crate.
  Graded against madge over v6/tsv2: recall 0.992, precision 0.988.
  --deps resolves import and export-from specifiers syntactically instead, with
  no indexer subprocess. Best effort; graded against the same oracle on the same
  corpus at recall 1.000 and precision 1.000, which measures agreement with
  another syntactic scanner and NOT correctness: the 9 edges --scip-deps has and
  madge lacks are inferred type references with no import statement, and no
  syntactic resolver can see them.

FILE FACT (--file-fact)
  Prepends one `file` row to the normal stream, carrying the content digest,
  byte count and line count. It rides the same read as extraction.

THE TWO NAMED FAMILIES (--family scip | --family diet_scip)
  DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA.
  --family scip ROOT ensures the root's SCIP index (an existing index wins; else
  the indexer its marker files name runs once under a wall budget, its whole
  process group killed on the deadline) and streams v5's scip_* relation shapes:
  scip_def, scip_name, scip_ref, scip_edge, scip_fn_edge, scip_callee_type,
  scip_local, scip_impl, behind one scip_index header row. Compiler-resolved.
  v5's scip_occurrence and scip_binding are NOT in that set. scip_occurrence is
  already a record tag on this wire (the byte-span passthrough row under
  --scip-facts) with different fields, and two shapes under one tag is exactly
  the silent drift the goldens exist to stop. Both are one consumer join off
  --scip-facts --scip-record scip_occurrence, which carries the spans and every
  role bit; scip_binding additionally wants the source slice at those spans.
  --family diet_scip PATH... runs this crate's own front-ends plus name-match
  resolution over the supplied files, emitting resolved_edge and
  resolved_type_edge. No indexer, no type checker, no index. It is wrong
  wherever a name is ambiguous corpus-wide, which is what the other name buys.
  A root that cannot be indexed emits scip_skip rows and exits 0: a missing
  toolchain skips a root without killing the caller, and without the silently
  empty stream that reads as 'this project has no symbols'.

PROJECT MODE (--resolve)
  `--resolve PATH...` runs phase 2 over the supplied files as one project.
  `--family call` (the default) emits `resolved_edge`; `--family type` emits
  `resolved_type_edge`; `--family call,type` emits both. Adding
  `--project-root DIR` with `--scip-index FILE` or `--scip-build` puts a SCIP
  index in the resolve context, which lets the call arm emit `scip_override`
  rows where the indexer disagrees with the name match.";
