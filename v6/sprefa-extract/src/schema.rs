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
  record=node   family=<cst|type|call|df|cfg>  span={start,end}   kind=<slug>   name=<string|null>
  record=edge   family=<cst|df|cfg>        kind=<slug>        from={start,end}  to={start,end}
  record=sig    family=type                owner={start,end}  owner_start=<u32>  owner_end=<u32>  slot=<param|ret>  pos=<u32>  ty=<name>
  record=param  family=df                  span={start,end}   pos=<u32>
  record=arg    family=df                  call={start,end}   pos=<i64>  arg={start,end}
  record=df_field  family=df               owner={start,end}  name=<string>  value={start,end}
  record=df_lit    family=df               node={start,end}   kind=<lit|template|concat>  text=<string>
  record=df_loop   family=df               span={start,end}   var=<string|null>  collection=<string|null>
  record=df_nest   family=df               call={start,end}   loop={start,end}   depth=<u32>  collection=<string|null>
  record=df_allocates  family=df           owner={start,end}
  record=site   family=call                span={start,end}   callee=<name>  callee_path=<string|null>
  record=method_owner  family=call         owner={start,end}  self_type=<string|null>  trait=<string|null>
  record=cfg_scope  family=call            span={start,end}   cfg=<string>   (only for cfg-guarded defs)
  record=test_only_call  family=call        callee=<name>  cfg=<string>   (only for callees every site in the file names under a cfg naming `test`)
  record=macro_site  family=call            span={start,end}   macro_name=<string>  source=<mbe|scip>
  record=reference  family=call            span={start,end}   functor=<name/arity>  position=<goal|head_arg|term_arg>
  record=const  family=type                owner={start,end}  field=<string|null>  text=<string>  kind=<lit|template>
  record=doc    family=type                owner={start,end}  parent=<string|null>  text=<string>
  record=doc_tag  family=type              owner={start,end}  tag=<string>  arg=<string|null>  text=<string>
  record=doc_node  family=type             span={start,end}   kind=<heading|code_block>  name=<string>  parent=<string|null>
  record=data_doc  family=data            ordinal=<u32>  span={start,end}  format=<json|jsonl|yaml|toml>  doc=<json value>
  record=data_value  family=data          ordinal=<u32>  path=<dotted>  kind=<object|array|string|number|boolean|null>  text=<string|null>  span={start,end}
  record=specifier  family=call            span={start,end}   name=<string>  kind=<slug>  module=<string|null>  imported=<string|null>
  record=unresolved  family=call           path=<string, only under --resolve>  span={start,end}   reason=<slug>  detail=<string>
  record=capture  query=<id>  capture=<name>  text=<string>  start=<u32>  end=<u32>  match_start=<u32>  match_end=<u32>
  record=resolved_edge  caller_path=<string>  caller_name=<string|null>  callee_path=<string>  callee_name=<string|null>  caller_site_start=<u32>  caller_site_end=<u32>  kind=<slug>
  record=resolved_type_edge  owner_path=<string>  owner_name=<string|null>  owner_start=<u32>  owner_end=<u32>  target_path=<string>  target_name=<string|null>  kind=<slug>
  record=resolved_import  src_path=<string>  name=<string>  local=<string>  target_path=<string>  target_name=<string|null>  kind=<slug>  hops=<u32>
  record=flow_edge  family=flow  kind=<slug>  from_blob=<hex>  from={start,end}  to_blob=<hex>  to={start,end}
  record=file_edge  src_path=<string>  dst_path=<string>  kind=<slug>  symbols=<u32>
  record=file_unresolved  src_path=<string>  module=<string>  reason=<slug>
  record=package_edge  src_manifest=<string>  dst_manifest=<string>  kind=<slug>
  record=file  path=<string>  digest=<hex>  bytes=<u32>  lines=<u32>
  record=size_skip  path=<string>  bytes=<u64>  limit=<u64>  reason=<over_max_bytes>
  record=scip_metadata  version=<i32>  tool_name=<string>  tool_version=<string>  tool_arguments=[<string>]  project_root=<string>  text_document_encoding=<i32>
  record=scip_document  path=<string>  language=<string>  position_encoding=<i32>  text=<string|null>
  record=scip_occurrence  path=<string>  symbol=<string>  start=<u32>  end=<u32>  roles=<i32>  definition=<bool>  import=<bool>  write_access=<bool>  read_access=<bool>  generated=<bool>  test=<bool>  forward_definition=<bool>  syntax_kind=<i32>  enclosing_start=<u32|null>  enclosing_end=<u32|null>  text=<string|null>
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
               call (callables + call sites), df (intra-procedural value flow),
               flow (inter-procedural value flow, derived),
               cfg (intra-procedural control flow, derived from cst).
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
  self_type    the impl self type a method def belongs to (null for a trait's
               own items).
  trait        the trait a method def implements or is declared in (null for an
               inherent impl).
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
  from_blob/to_blob  the content keys (hex) of the two files a flow edge
               crosses; `from`/`to` are byte spans into those files.
  bytes        the file's length in bytes.
  lines        the file's line count as an editor shows it: an unterminated last
               line still counts, an empty file is 0.
  module       a specifier's source module as written, null when the language
               puts the module in `name` (path-only forms).
  imported     the name the SOURCE module spells for a binding, when the local
               name renames it (`import {inner as local}`, a default import's
               `default`); null when the two agree or when the module path's
               trailing segment already spells it (rust, go, kotlin, dl6,
               prolog). v5's module_binding imported column.
  reason       why a runtime-computed edge marker fired (see its vocabulary).
  detail       the computed expression's source text, exactly the text at `span`.
  symbol       a SCIP symbol string; `local `-prefixed symbols are document-scoped.
  roles        the raw scip.proto SymbolRole bitfield, kept whole.
  definition / import / write_access / read_access / generated / test /
  forward_definition  the seven SymbolRole bits, one column each.
  syntax_kind  raw scip.proto SyntaxKind ordinal (0 = unspecified).
  enclosing_start/enclosing_end  the nearest enclosing AST node's byte span,
               null when the indexer emitted none.
  text         on scip_occurrence only, the source slice at the occurrence's
               byte span (lossy-utf8), carried only under --occurrence-text and
               absent (not null) when the flag is off or the span is past EOF.
  display_name the symbol's name as scip records it.
  enclosing_symbol  the owner of a local symbol; empty for global symbols.
  related_symbol  the other end of a scip.proto Relationship.
  ref_symbol   a symbol referenced inside a signature's text; the start/end on
               that record are offsets into the SIGNATURE TEXT, not a document.
  severity/tags  raw scip.proto Severity and DiagnosticTag ordinals.
  src_path/dst_path  the two ends of a file dependency edge.
  symbols      how many distinct symbols cross one file edge of that kind.
  src_manifest/dst_manifest  the two ends of a package edge: project-relative
               manifest paths, never package names, so a package edge joins a
               file edge on the same key shape.

KIND VOCABULARIES (the `kind` field)
  type node   struct enum trait class interface alias function method const
  call node   function method lambda
  df node     param let_bind var_read var_write lit call_res new member ret
              borrow binop unop loop if match block closure try break expr
              cond logic concat template
  cst node    the grammar node type as named by ast-grep / tree-sitter (open set)
  cst edge    child
  df edge     direct
  df lit kind  lit (cooked literal) | template | concat (raw source slice)
  flow edge   arg_to_param | ret_to_call_res | lambda_elem | lambda_ret
  cfg node    entry exit stmt branch loop jump ret
  cfg edge    next arm jump exit
  const kind  lit (cooked literal) | template (raw source slice, holes intact)
  sig slot    param | ret
  unresolved reason  dynamic-import | computed-member-call | spread-call-args
                    (phase 1) | no_corpus_def | ambiguous | builtin | inferred |
                    external (--resolve, one row per call site or import spec
                    the resolve arm dropped)
  specifier kind    named | default | namespace | side_effect | reexport |
                    include | reexport_module | dynamic_import | require
  file_edge kind    the specifier kind that bound the crossing, or `unknown`
                    under --scip-deps: an index records resolved occurrences,
                    never the import statement that bound the name, so the form
                    is not in it to read.
  file_unresolved reason  the deps.rs Policy slug that stopped the specifier:
                    node_modules_boundary | absolute_path | relative_unresolved
  package_edge kind  Cargo.toml: normal | dev | build. package.json:
                    normal | dev | peer. go.mod: require | replace.
  resolved_edge kind       name_resolve | scip_override | value_ref |
                    import_resolve (the callee is an import binding, bound
                    through the language's own module plane)
  resolved_import kind     local (the target module declares the name) |
                    indirect (>=1 `export { x } from` hop) | star (>=1
                    `export *` hop) | namespace (a module object, not one
                    declaration) | default (the module's default export).
                    Precedence when several apply: namespace, star, indirect,
                    default, local.
  resolved_type_edge kind  field | impl | variant | generic | uses | doc_ref
  doc_node kind            heading | code_block

CONTROL FLOW (--family cfg)
  Intra-procedural only, and DERIVED from the cst family rather than projected
  by a language front-end: `--family cfg` turns the cst mask on and emits cfg
  node and edge records beside it. Every callable gets an entry and an exit
  node, both carrying the callable's own span, so the `kind` field is what
  separates them. A `next` edge whose destination starts before its source is a
  loop back edge. Jump targets outside a loop (goto, labelled break) are NOT
  resolved, so those nodes have no outgoing edge. Languages with kind_role rows:
  rust, go, ts, kotlin; any other language emits no cfg rows at all.
  Post-dominance and the CDG edge color are NOT built.

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
  regardless of which resolver filled it. The edge key is (src, dst, kind): one
  pair carries one row per import form, and `symbols` counts the distinct names
  of that form.
  --scip-deps folds a SCIP index: the indexer already resolved every reference,
  so the graph falls out of the index with no module resolver in the crate.
  Graded against madge over v6/tsv2: recall 0.992, precision 0.988.
  --deps also emits file_unresolved for every specifier that resolved to
  nothing, carrying the policy that stopped it, so a package boundary, an
  absolute path and a broken relative path stay three answers.
  --deps resolves import and export-from specifiers syntactically instead, with
  no indexer subprocess. Best effort; graded against the same oracle on the same
  corpus at recall 1.000 and precision 1.000, which measures agreement with
  another syntactic scanner and NOT correctness: the 9 edges --scip-deps has and
  madge lacks are inferred type references with no import statement, and no
  syntactic resolver can see them.

MODULE PLANE (--resolve)
  Module resolution is the LANGUAGE'S OWN algorithm, run once per file set as
  its own plane, and every resolve arm binds an imported name through it.
  Name-matching a callee across files is the fallback for names with NO import
  binding, never the first leg.
  ts/js runs ECMA-262 16.2.1.6.3 ResolveExport over `oxc_parser`'s ModuleRecord,
  with `oxc_resolver` turning each specifier into a corpus file: local exports,
  `export { x } from` re-exports (renaming included), `export *` barrels to any
  depth, `export * as ns`, default exports, `import * as ns` member calls, and
  the TS forms `import x = require(...)` and `export =`. A re-export cycle
  terminates on the spec's own resolveSet.
  It emits one resolved_import row per import binding per file, and the
  call edges it binds carry kind=import_resolve.
  Two `export *` arms offering DIFFERENT bindings for one name is the spec's
  AMBIGUOUS outcome: no edge, and an `unresolved` row with reason=ambiguous.
  rust runs the Rust Reference's own name resolution over its `use`/`mod`
  facts (a dedicated second parse, `src/lang/rust_modules.rs`): `crate::`/
  `self::`/`super::`/absolute paths to a home file (reusing the kink-4
  qualifier-to-file match), `pub use` re-export chains to any depth, `use
  a::*` globs (ambiguous on disagreement, precedence namespace > star >
  indirect > local, `default` never occurs), and `use a::b;` where `b` is a
  module rather than an item (namespace). A local def always shadows an
  import. go runs its own directory-scoped plane (`src/lang/go_modules.rs`):
  each import spec resolves to the target directory's REAL package name (its
  `package` clause, never the import path's last segment), kind=local, or
  kind=namespace for a dot import; an unresolvable directory emits an
  unresolved row reason=external instead. A `pkg.Name` call or type reference
  binds through the plane at kind=import_resolve, exported names only (Go's
  own capitalization rule); a bare name may still bind through a dot import
  after same-file/corpus-unique matching declines, so a local declaration
  always shadows it. Interface and receiver dispatch (PR #554) are unchanged.

PACKAGE EDGES (--package-deps)
  Reads the supplied manifests and emits package_edge rows for the dependency
  pairs that stay inside the corpus: a dependency is an edge only when another
  SUPPLIED manifest declares that name. One arm per manifest kind (Cargo.toml,
  package.json, go.mod); a path that is not a manifest is skipped, and an
  unparseable manifest contributes no name and no edges rather than an error.
  It is v5's crate_edge generalized: that relation was Cargo-only and keyed on
  crate names.

FILE FACT (--file-fact)
  Prepends one `file` row to the normal stream, carrying the content digest,
  byte count and line count. It rides the same read as extraction.

SIZE CEILING (--max-bytes)
  A single input over the ceiling is NOT parsed. It emits one size_skip row
  naming the path, the byte count and the ceiling, and exits 0. The default is
  16777216; --max-bytes N sets it, --max-bytes 0 removes it. The decision is
  made on the file size before any parse, so it covers the normal family
  stream, --bench, and --ast-pattern alike, and --file-fact still prepends its
  identity row (a digest over bytes already read, not the cost being bounded).
  A whole-project mode (--resolve, --deps, --scip-*) takes directories and sets
  and is not covered.
  The point is that a silent timeout is a defect and a named skip is a fact.
  A caller that gets rc=124 and an empty stream cannot tell it from a file with
  no facts, and learns neither which file nor how big. The ceiling is measured:
  16777216 skips exactly one file in a 77,472-file rust corpus (a 29,328,358 B
  machine-generated parser table that costs 12.55 s and 3.0 GB, all of it parse
  time), no ts/js corpus file, and no fixture in this crate.

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
  role bit; scip_binding's source-slice need is answered by that row's optional
  `text` field under --occurrence-text (issue extract-scip-vocab-occurrence-binding).
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
