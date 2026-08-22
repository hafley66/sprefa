# v5 extraction parity

Measured 2026-08-21 at `6967750a7`. Every row cites a `path:line` on both sides.
No row is an opinion about what an engine "supports"; where v6 stops, the stop
is a throw site or a missing fixture, named.

## Contents

- [How to read a row](#how-to-read-a-row)
- [Counts](#counts)
- [The one finding](#the-one-finding)
- [1. Built-in relations, by plane](#1-built-in-relations-by-plane)
  - [1.1 type](#11-type)
  - [1.2 call](#12-call)
  - [1.3 dataflow](#13-dataflow)
  - [1.4 module / import](#14-module--import)
  - [1.5 scip](#15-scip)
  - [1.6 doc](#16-doc)
  - [1.7 comment, template, unresolved, spine, node](#17-comment-template-unresolved-spine-node)
  - [1.8 git / repo / corpus](#18-git--repo--corpus)
  - [1.9 engine-internal relations (no extraction content)](#19-engine-internal-relations-no-extraction-content)
- [2. Extraction ops](#2-extraction-ops)
- [3. Languages](#3-languages)
- [4. SCIP indexers](#4-scip-indexers)
- [5. Cost notes](#5-cost-notes)
- [6. Prior art this replaces or extends](#6-prior-art-this-replaces-or-extends)

## How to read a row

A v5 capability lands in v6 across TWO seams, and both have to be green before a
`.dl` rail can be rewritten as a `.dl6` rail:

| seam | question | where it is answered |
|---|---|---|
| RECORD | does `sprefa-extract` emit the fact | `v6/sprefa-extract/src/schema.rs`, the `tests/*.rs` battery |
| DOOR | can a `.dl6` program on the Rust runtime ask for it | `v6/prolog/compile/registry.pl` host names + `v6/sprefa-engine-rs/src/hosts.rs` `executor_for` |

Parity words, applied to the RECORD seam:

| word | meaning |
|---|---|
| identical | same facts, column-for-column modulo the byte-span vs line/col convention |
| superset | v6 carries every v5 column plus more |
| subset | v6 drops a named column or a named case |
| missing | no v6 record carries it |

The DOOR column is `yes` / `no` / `n/a`, and `no` always cites the stop.

## Counts

RECORD seam, over the 112 v5 built-in relation declarations:

| bucket | rels |
|---|---|
| identical | 56 |
| superset | 13 |
| subset | 8 |
| missing | 12 |
| n/a (engine-internal, no extraction content) | 23 |

Per plane, so the totals are checkable:

| plane | rels | identical | superset | subset | missing | n/a |
|---|---|---|---|---|---|---|
| 1.1 type | 11 | 6 | 3 | 2 | 0 | 0 |
| 1.2 call | 7 | 3 | 3 | 0 | 0 | 1 |
| 1.3 dataflow | 15 | 13 | 1 | 1 | 0 | 0 |
| 1.4 module | 10 | 4 | 4 | 2 | 0 | 0 |
| 1.5 scip | 10 | 9 | 1 | 0 | 0 | 0 |
| 1.6 doc | 2 | 2 | 0 | 0 | 0 | 0 |
| 1.7 comment/template/unresolved/spine/node | 7 | 3 | 0 | 2 | 0 | 2 |
| 1.8 git/repo/corpus | 22 | 16 | 1 | 1 | 2 | 2 |
| 1.9 engine-internal | 28 | 0 | 0 | 0 | 10 | 18 |
| **total** | **112** | **56** | **13** | **8** | **12** | **23** |

DOOR seam, over the 84 relations in planes 1.1 to 1.8, measured WITH the
`ast_rule` host:

| bucket | rels |
|---|---|
| reachable from a `.dl6` program today | 57 |
| record exists, the door does not reach it | 20 |
| neither the record nor the door | 2 |
| n/a (v5 daemon plumbing) | 5 |

The two that moved are `comment_node` and `template_parts`: their spans were
always on the cst wire and their `text` now comes off `ast_rule`.

Extraction ops (v5 `op_docs()` source ops, 11 of them). Re-measured after the
`ast_rule` host landed on main at `3da1100f2` / `cd58a6917`:

| bucket | ops |
|---|---|
| reachable from a `.dl6` program | 9 (`scan`, `json`, `jsonp`, `match_line`, `match`, `match_ast`, `sg`, `ast_yaml`, `comment`) |
| compile path exists, the door does not reach it | 1 (`ast`, the tree-sitter s-expression form) |
| no v6 equivalent, by decision | 1 (`cmd`) |

## The one finding

**v6 extracts more than v5 and its door asks for less — but the biggest half of
that gap closed while this census was being written.**

`ast_rule` landed on main at `3da1100f2` (`v6/extract: wire typed ast-grep rules
through DL6 hosts`) and `cd58a6917`. It is a linked in-process executor
(`v6/sprefa-engine-rs/src/hosts.rs:48`) answering `AstRuleMatch` rows whose
`captures` carry `name`, **`text`** and `span`
(`v6/sprefa-extract/src/lang/1_ast_rule.rs:76-91`), with the whole ast-grep rule
algebra behind it: `Pattern`, `Kind`, `Regex`, `Matches`, `All`, `Any`, `Not`,
`Inside`, `Has`, `Follows`, `Precedes` with `stopBy`, plus `fix` producing an
`AstRuleMutationProposal`.

That is v5's `match_ast`, `sg`, `ast_yaml`, `match_line`, `match` and `comment`
in one host, and it is what let the `no-new-eprintln` rail port land in this PR
after this document first recorded it as blocked. Every count below is measured
against the tree WITH that host, and the sections that follow keep the older
per-relation reading where it is still true.

The residue is real and smaller:

Every RECORD-seam gap `@extract-port-closeout` opened is closed but three, and
the three left are all in one arm (python). The extractor is at or past parity
on facts.

The DOOR is where v5 is still alive. A `.dl6` program on the Rust runtime can
declare an `sh` host, and the host's command line is parsed by
`SprefaExtractExecutor::run` at `v6/sprefa-engine-rs/src/hosts.rs:1046-1126`. That
parser accepts exactly two flags:

```
hosts.rs:1061   --file-fact
hosts.rs:1063   --family <cst|type|call|df|data>
hosts.rs:1071   anything else starting with `--`  ->  "flag `{token}` is not linked in-process"
hosts.rs:1113   --family scip | diet_scip         ->  "mode `{}` is not linked in-process"
hosts.rs:1113   --family cfg                      ->  "family `cfg` is not a known family"
```

So `--resolve`, `--deps`, `--scip-deps`, `--package-deps`, `--scip-facts`,
`--occurrence-text`, `--family cfg`, `--family scip` and `--family diet_scip`
are all CLI-reachable and dl6-unreachable. (`--ast-pattern` and its two siblings
are also refused here, but the `ast_rule` host answers the same question with a
richer rule model, so the CLI flags are a convenience rather than the door.)

The cst FAMILY still gives node kind and span with
`name` null for every rust node
(`v6/sprefa-extract/src/lang/astgrep.rs:168-200`, the `CstProjector` never fills
`name`), so a dl6 program cannot read the identifier at a span. Probed
2026-08-21 on a rust file holding `eprintln!("x")`:

```
extract --family cst probe.rs
  {"record":"node","family":"cst","span":{"start":13,"end":27},"kind":"macro_invocation","name":null}
  {"record":"node","family":"cst","span":{"start":13,"end":21},"kind":"identifier","name":null}
extract --family call probe.rs
  (no site record: the rust front-end does not project macro invocations as calls)
```

so a program that wants text asks `ast_rule` rather than `--family cst`. That
is a design, not a gap: the cst wire stays span-only and 48.5MB-class text never
rides it.

Child issues, all re-measured: `@dl6-ts-query-executor` (now the `ast`
tree-sitter arm alone), `@dl6-cfg-family-unlinked`, `@dl6-scip-facts-door`,
`@dl6-deps-package-door`.

## 1. Built-in relations, by plane

The v5 inventory is `all_builtin_decls()` at `src/engine/decls.rs:26-56`, which
chains 25 per-family decl functions plus `crate::rels::rel_kind_decls()`
(`src/rels/mod.rs:179`). 112 declarations. It is the same list `rel_catalog`
projects, so this table and v5's own self-description cannot drift.

### 1.1 type

v6 record shapes: `v6/sprefa-extract/src/schema.rs:23-41`.

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `type_entity(repo, sym, name, kind, parent, file, line)` | `src/engine/decls.rs:522` | `record=node family=type` (`span`, `kind`, `name`) | `schema.rs:23` | superset (byte spans, not line) | yes, `sh type_node_at` | `tests/golden_parity.rs` PORTED `type_node` |
| `type_entity_rev(…, rev)` | `src/engine/decls.rs:526` | rev is the demand's `digest` input, never a column | `registry.pl:342` | identical in effect | yes | `tests/27_blob_cache.rs` |
| `type_edge(from, to, kind, repo)` | `src/engine/decls.rs:517` | `record=resolved_type_edge` (`owner_path`, `target_path`, `kind`) | `schema.rs:48` | superset (carries both file paths + spans) | **no**, needs `--resolve` | `tests/golden_parity.rs` 4b-iii/4d-i-go/4d-i-rust |
| `type_edge_rev(…, rev)` | `src/engine/decls.rs:520` | same, digest-keyed | `schema.rs:48` | identical in effect | **no** | same |
| `type_sig(sym, slot, pos, ref)` | `src/engine/decls.rs:530` | `record=sig family=type` (`owner`, `slot`, `pos`, `ty`) | `schema.rs:25` | superset (owner span + flat owner bytes) | yes, `sh sig_at` | `tests/golden_parity.rs` PORTED `type_sig` |
| `type_link(src, dst, kind)` | `src/engine/decls.rs:533` | `record=resolved_type_edge` under `--scip-index` | `schema.rs:48` | subset: v5's link is SCIP sym-to-sym; v6 keys on file path + name, so a symbol with no file loses its row | **no**, needs `--resolve --scip-index` | `tests/1_resolve_cli.rs` |
| `type_link_rev(…, rev)` | `src/engine/decls.rs:535` | same | `schema.rs:48` | subset | **no** | same |
| `doc_comment(repo, sym, line, text)` | `src/engine/decls.rs:543` | `record=doc family=type` (`owner`, `parent`, `text`) | `schema.rs:39` | identical | yes, `sh type_node_at` | `tests/19_docs_lang_arms.rs`, `golden_parity.rs` `rust_doc_parity` |
| `doc_tag(repo, sym, tag, arg, text)` | `src/engine/decls.rs:546` | `record=doc_tag family=type` (`owner`, `tag`, `arg`, `text`) | `schema.rs:40` | identical | yes | `tests/19_docs_lang_arms.rs` |
| `const_value(repo, sym, field, text, kind, file, line)` | `src/engine/decls.rs:815` | `record=const family=type` (`owner`, `field`, `text`, `kind`) | `schema.rs:38` | identical | yes | `tests/golden_parity.rs` PORTED `const_value` |
| `const_value_rev(…, rev)` | `src/engine/decls.rs:820` | same, digest-keyed | `schema.rs:38` | identical in effect | yes | same |

Plane verdict: RECORD identical or superset for all 11. DOOR reaches 6 of 11;
the 5 it does not reach are the phase-2 (resolved) rows, all behind `--resolve`.

### 1.2 call

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `call_def(repo, sym, kind, file, line, end)` | `src/engine/decls.rs:595` | `record=node family=call` (`span`, `kind`, `name`) | `schema.rs:23` | superset: v5's `line`/`end` are the same span in bytes; v6 adds `method_owner` | yes, `sh call_node_at` | `tests/golden_parity.rs` PORTED `call_def` |
| `call_def_rev(…, rev)` | `src/engine/decls.rs:599` | digest-keyed | `schema.rs:23` | identical in effect | yes | `tests/27_blob_cache.rs` |
| `call_site(repo, caller, callee, file, line)` | `src/engine/decls.rs:603` | `record=site family=call` (`span`, `callee`, `callee_path`) | `schema.rs:33` | superset: v5's `caller` is derivable by span containment against the `node` rows, and v6 adds `callee_path` | yes, `sh extract` / `call_ref` | `tests/golden_parity.rs` PORTED `call_site` |
| `call_edge(caller, callee, kind)` | `src/engine/decls.rs:607` | `record=resolved_edge` (`caller_path`, `caller_name`, `callee_path`, `callee_name`, `kind`) | `schema.rs:47` | superset (both file paths, the call-site span) | yes via `sh /scip/call` / `sh /scip/diet/call` | `tests/8_scip_families_cli.rs`, `v6/dl/deadcode/receiver-rail.dl6` |
| `call_edge_rev(…, rev)` | `src/engine/decls.rs:610` | digest-keyed | `schema.rs:47` | identical in effect | yes | same |
| `call_name(sym, name)` | `src/engine/decls.rs:617` | the `name` column on `record=node family=call` | `schema.rs:23` | identical (v5's sym-to-name side table is one column of the node row) | yes | `tests/golden_parity.rs` |
| `call_kind(fn, kind)` | `src/engine/decls.rs:625` | not extraction: v5 computes it in the engine over `call_site` | `src/engine/family/call_kind.rs:2-6` | n/a | n/a | closed no-code-owed in `@extract-port-closeout` |

Plane verdict: RECORD superset or identical for all 7. DOOR reaches all 6
extraction rows.

### 1.3 dataflow

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `df_node(id, kind, var, fn, file, line, col)` | `src/engine/decls.rs:639` | `record=node family=df` (`span`, `kind`, `name`) | `schema.rs:23` | superset: v5's `id` is a `file:line:col:kind` join handle, v6's is the span itself; `var`/`fn` come off the containing node by span | yes, `sh df_node_at` | `tests/golden_parity.rs` PORTED `df_node` (byte-exact) |
| `df_node_rev(…, rev)` | `src/engine/decls.rs:655` | digest-keyed | `schema.rs:23` | identical in effect | yes | `tests/27_blob_cache.rs` |
| `df_node_repo(id, repo)` | `src/engine/decls.rs:674` | `repo` is a host INPUT on `repo_extract`, returned on the response row | `registry.pl:350` | identical in effect | yes | `v6/dl/fixtures/crawl_org.dl6` |
| `df_node_repo_rev(…, rev)` | `src/engine/decls.rs:685` | same | `registry.pl:350` | identical in effect | yes | same |
| `df_edge(from, to)` | `src/engine/decls.rs:690` | `record=edge family=df kind=direct` | `schema.rs:24` | identical | yes, `sh df_edge_at` | `tests/golden_parity.rs` PORTED `df_edge` (byte-exact) |
| `df_param(id, pos)` | `src/engine/decls.rs:725` | `record=param family=df` (`span`, `pos`) | `schema.rs:26` | identical | yes, `sh df_param_at` | `tests/12_df_identity.rs` |
| `df_arg(call, pos, arg)` | `src/engine/decls.rs:737` | `record=arg family=df` (`call`, `pos`, `arg`) | `schema.rs:27` | identical | yes, `sh df_arg_at` | `tests/12_df_identity.rs` |
| `df_arg_rev(…, rev)` | `src/engine/decls.rs:750` | digest-keyed | `schema.rs:27` | identical in effect | yes | same |
| `df_field(id, field, value)` | `src/engine/decls.rs:765` | `record=df_field family=df` (`owner`, `name`, `value`) | `schema.rs:28` | identical | yes | `tests/18_df_aux_fields_lits.rs` |
| `df_field_rev(…, rev)` | `src/engine/decls.rs:778` | digest-keyed | `schema.rs:28` | identical in effect | yes | same |
| `df_lit(id, text, kind)` | `src/engine/decls.rs:790` | `record=df_lit family=df` (`node`, `kind`, `text`) | `schema.rs:29` | identical | yes | `tests/18_df_aux_fields_lits.rs` |
| `df_lit_rev(…, rev)` | `src/engine/decls.rs:799` | digest-keyed | `schema.rs:29` | identical in effect | yes | same |
| `loop_over(file, start, end, var, collection, fn)` | `src/engine/decls.rs:696` | `record=df_loop family=df` (`span`, `var`, `collection`) | `schema.rs:30` | subset: no `fn` column; the enclosing callable is a span-containment join against `record=node family=call` | yes | `tests/23_df_aux_loops_nests.rs` |
| `nest(call_id, loop_id, depth, collection)` | `src/engine/decls.rs:714` | `record=df_nest family=df` (`call`, `loop`, `depth`, `collection`) | `schema.rs:31` | identical | yes | `tests/23_df_aux_loops_nests.rs` |
| `allocates(fn)` | `src/engine/decls.rs:703` | `record=df_allocates family=df` (`owner`) | `schema.rs:32` | identical | yes | `tests/23_df_aux_loops_nests.rs` |

Plane verdict: RECORD identical or superset for 14, subset for 1 (`loop_over`,
one derivable column). DOOR reaches all 15. This is the plane v6 has finished.

v6-only, no v5 twin: `record=flow_edge family=flow` (interprocedural
`arg_to_param` / `ret_to_call_res` / `lambda_elem` / `lambda_ret`,
`schema.rs:49`), pinned by `tests/13_flow_join.rs` and
`tests/23_flow_cli_dispatch.rs`.

### 1.4 module / import

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `module_import(file, rev, specifier, kind, line)` | `src/engine/decls.rs:470` | `record=specifier family=call` (`span`, `name`, `kind`, `module`, `imported`) | `schema.rs:44` | superset (adds `imported`, the renaming source name) | yes, `sh specifier_at` | `tests/24_rust_specifiers.rs` |
| `module_edge(src, dst)` | `src/engine/decls.rs:473` | `record=file_edge` (`src_path`, `dst_path`, `kind`, `symbols`) | `schema.rs:50` | superset (adds edge kind and a crossing count) | **no**, needs `--deps` or `--scip-deps` | `tests/7_diet_deps_cli.rs` |
| `module_edge_rev(…, rev)` | `src/engine/decls.rs:475` | same | `schema.rs:50` | superset | **no** | same |
| `module_unresolved(file, specifier, reason, line)` | `src/engine/decls.rs:480` | `record=file_unresolved` (`src_path`, `module`, `reason`) | `schema.rs:51` | identical | **no**, needs `--deps` | `tests/7_diet_deps_cli.rs` |
| `module_unresolved_rev(…, rev)` | `src/engine/decls.rs:484` | same | `schema.rs:51` | identical | **no** | same |
| `module_binding(file, local_name, source_module, imported_name, kind)` | `src/engine/decls.rs:503` | `record=specifier` `name`/`module`/`imported`/`kind` | `schema.rs:44` | identical | yes | `tests/24_rust_specifiers.rs` |
| `module_binding_rev(…, rev)` | `src/engine/decls.rs:498` | digest-keyed | `schema.rs:44` | identical in effect | yes | same |
| `module_binding_resolved(file, local, source, dst)` | `src/engine/decls.rs:494` | `record=file_edge` joined to `record=specifier` on the file | `schema.rs:50` | subset: no per-BINDING resolution row, only the per-FILE edge with a `symbols` count | **no** | `tests/7_diet_deps_cli.rs` |
| `module_binding_resolved_rev(…, rev)` | `src/engine/decls.rs:489` | same | `schema.rs:50` | subset | **no** | same |
| `crate_edge(src, dst, kind, rev)` | `src/engine/decls.rs:487` | `record=package_edge` (`src_manifest`, `dst_manifest`, `kind`) | `schema.rs:52` | superset: v5 was Cargo-only and keyed on crate NAMES, v6 keys on manifest paths and covers `Cargo.toml` / `package.json` / `go.mod` | **no**, needs `--package-deps` | `tests/28_package_edges.rs` |

Plane verdict: RECORD superset or identical for 8, subset for 2. DOOR reaches
3 of 10 (`module_import`, `module_binding`, `module_binding_rev`, all three the
same `specifier` record). The whole file-and-package edge plane is CLI-only.
Issue `@dl6-deps-package-door`.

The `specifier` record is emitted for rust, go, kotlin, ts, dl6 and prolog
(`schema.rs:119-121` names the languages whose module path already spells the
name). Issue `@extract-module-plane-non-ts` is still open for the resolver arm.

### 1.5 scip

v5's ten: `src/rels/scip.rs:61-88`. v6's `--family scip` door: eight, named at
`v6/sprefa-extract/src/schema.rs:239-252`.

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `scip_def(symbol, file, repo)` | `src/rels/scip.rs:61` | `record=scip_def` | `schema.rs:66` | identical | **no**, `--family scip` not linked in-process | `tests/8_scip_families_cli.rs` |
| `scip_name(symbol, name)` | `src/rels/scip.rs:63` | `record=scip_name` | `schema.rs:67` | identical | **no** | same |
| `scip_ref(file, symbol, def_file, repo)` | `src/rels/scip.rs:65` | `record=scip_ref` | `schema.rs:68` | identical | **no** | same |
| `scip_edge(src, dst, repo)` | `src/rels/scip.rs:67` | `record=scip_edge` | `schema.rs:69` | identical | **no** | same |
| `scip_fn_edge(caller, callee)` | `src/rels/scip.rs:69` | `record=scip_fn_edge` | `schema.rs:70` | identical | **no** | same |
| `scip_callee_type(sym, type)` | `src/rels/scip.rs:71` | `record=scip_callee_type` | `schema.rs:71` | identical | **no** | same |
| `scip_local(fn, name)` | `src/rels/scip.rs:73` | `record=scip_local` | `schema.rs:72` | identical | **no** | same |
| `scip_impl(impl, iface)` | `src/rels/scip.rs:75` | `record=scip_impl` | `schema.rs:73` | identical | **no** | same |
| `scip_occurrence(file, symbol, line, col, end_line, end_col, role, repo)` | `src/rels/scip.rs:77` | `record=scip_occurrence` under `--scip-facts` | `schema.rs:56` | superset: byte spans instead of line/col, all seven role bits instead of one `role` string | **no**, `--scip-facts` not linked in-process | `tests/5_scip_facts_cli.rs` |
| `scip_binding(file, symbol, local_name, line, col, repo)` | `src/rels/scip.rs:83` | `record=scip_occurrence` + `--occurrence-text` | `schema.rs:56`, `schema.rs:131-133` | identical only with the flag: `local_name` is the source slice at the span | **no**, `--occurrence-text` not linked in-process | `tests/6_occurrence_text_cli.rs` |

Plane verdict: RECORD identical or superset for all 10. That resolves the
`8/10 rels ported` note in `chat_log/20260816.2.…:68` — with `--scip-facts` in
the picture it is 10/10 on the wire. DOOR reaches **zero** of the ten. What a
dl6 program CAN reach is the four resolved namespaces
(`scip__call`, `scip__diet__call`, `scip__type`, `scip__diet__type`,
`registry.pl:499-502`), which answer `resolved_edge` and `resolved_type_edge`
only. Issue `@dl6-scip-facts-door`.

### 1.6 doc

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `doc_node(repo, file, line, kind, name, parent)` | `src/engine/decls.rs:833` | `record=doc_node family=type` (`span`, `kind`, `name`, `parent`) | `schema.rs:41` | identical | yes, `sh type_node_at` | `tests/22_doc_node.rs` |
| `doc_ref(repo, file, line, sym, kind, matched_name)` | `src/engine/decls.rs:845` | `record=resolved_type_edge kind=doc_ref` | `schema.rs:48`, `schema.rs:172` | identical | **no**, needs `--resolve` | `tests/22_doc_node.rs` |

The CLAUDE.md open row "sprefa-extract has no markdown extractor" is STALE:
`MarkdownSource` is in the roster at `v6/sprefa-extract/src/lang/mod.rs:51` and
`source_for(".md")` returns it. Fix that line when this lands.

### 1.7 comment, template, unresolved, spine, node

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR | fixture |
|---|---|---|---|---|---|---|
| `comment_node(path, line, col, end_line, end_col, text, kind)` | `src/engine/decls.rs:556` | `record=node family=cst` for the span; `sh ast_rule` `kind: line_comment` for span AND text | `schema.rs:23`, `1_ast_rule.rs:87-91` | identical across the two doors | yes | `v6/dl/rails/no-new-eprintln-rail.dl6`, graded by `just no-new-eprintln` |
| `template_parts(file, line, node, idx, kind, text)` | `src/engine/decls.rs:576` | cst `template_string` / `template_substitution` / `string_fragment` nodes for the spans; `ast_rule` `kind:` for the text | `schema.rs:23-24`, `1_ast_rule.rs:87-91` | identical across the two doors | yes | closed no-code-owed in `@extract-port-closeout`; no fixture — write one |
| `unresolved(file, line, reason, detail)` | `src/engine/decls.rs:586` | `record=unresolved family=call` (`span`, `reason`, `detail`) | `schema.rs:45` | identical, and `detail` IS the source text at the span (`schema.rs:123`) | yes | `tests/20_unresolved.rs` |
| `string(id, text, norm)` | `src/engine/decls.rs:855` | engine-side interner view, not extraction | `v6/prolog/compile/…` `__str` table | n/a | n/a | — |
| `ref(id, string, file, lo, hi)` | `src/engine/decls.rs:857` | same | — | n/a | n/a | — |
| `node(id, kind, file, lo, hi, parent)` | `src/engine/decls.rs:866` | `record=node family=cst` | `schema.rs:23` | identical (`parent` is the `child` edge) | yes | `tests/4_capability_parity.rs` roster leg |
| `child(parent, child)` | `src/engine/decls.rs:882` | `record=edge family=cst kind=child` | `schema.rs:24` | identical | yes | same |

`unresolved` is worth calling out: it is the ONE v6 record that carries source
TEXT at a span (`detail`). It exists for three named cases only
(`dynamic-import`, `computed-member-call`, `spread-call-args`,
`schema.rs:161`), so it is not a general text door.

### 1.8 git / repo / corpus

These are not `sprefa-extract`'s job. In v6 they are engine hosts.

| v5 rel | v5 site | v6 equivalent | v6 site | RECORD | DOOR |
|---|---|---|---|---|---|
| `repo(slug, root, url)` | `src/engine/decls.rs:8` | `sh repos` / `sh gh_repos` org fan-out | `registry.pl:359,362` | identical | yes |
| `rev(id, repo, oid, ts)` | `src/engine/decls.rs:13` | the `digest` freshness input | `registry.pl:342` | identical in effect | yes |
| `content(id, hash)` | `src/engine/decls.rs:15` | `record=file` (`digest`, `bytes`, `lines`) under `--file-fact` | `schema.rs:53` | superset | yes |
| `file(repo, rev, path, content)` | `src/engine/decls.rs:17` | `sh files` / `sh repo_files`, `SoopyFilesExecutor` | `hosts.rs:251-302` | identical | yes |
| `file_lines(repo, path, rev, line_count)` | `src/rels/filelines.rs:28` | `record=file.lines` under `--file-fact` | `schema.rs:53` | identical | yes |
| `true()` | `src/engine/decls.rs:6` | `surface(true/0, guard, …, live)` | `registry.pl:140` | identical | yes |
| `changed(path)` | `src/rels/git.rs:28` | soopy change facts | closed by `@v5-source-workload-ports` (PR #293) | identical | yes |
| `changed_line(path, line)` | `src/rels/git.rs:94` | same | same | identical | yes |
| `created(path, name, email, ts)` | `src/rels/git.rs:494` | none | — | **missing** | no |
| `git_ref(repo, refname, kind, sha)` | `src/rels/git.rs:197` | ref/tag/merge-base/ancestry hosts | closed by `@v5-source-workload-ports` (PR #290) | identical | yes |
| `rev_behind(repo, refname, upstream, behind, ahead)` | `src/rels/git.rs:316` | same | same | identical | yes |
| `head(repo, name, oid)` | `src/engine/decls.rs:906` | same | same | identical | yes |
| `rev_advanced(repo, name, old, new)` | `src/engine/decls.rs:917` | derivable from two `head` rows across ticks | — | subset | yes |
| `checkout(repo, branch, pr_heads)` | `src/engine/decls.rs:367` | `sh repo_checkout` | `registry.pl:414` | identical | yes |
| `checkout_done`, `checkout_plan` | `src/engine/decls.rs:402,404` | the host's own response rows | `registry.pl:414` | identical | yes |
| `scip_want(repo)` | `src/engine/decls.rs:348` | `sh /scip/call` demand; the index is ensured by `ScipNamespaceExecutor` | `hosts.rs:759-796` | identical | yes |
| `rev_cmp_want(repo, refname, upstream)` | `src/engine/decls.rs:352` | closed by `@v5-source-workload-ports` | — | identical | yes |
| `program(path, hash, mtime)` | `src/engine/decls.rs:895` | daemon bookkeeping; dies with v5 | — | n/a | n/a |
| `def_target(name, file, line, kind)` | `src/engine/decls.rs:357` | LSP sink; see `plans/2026-08-12-v6-native-lsp.PLAN.md` | — | **missing** | no |
| `effect_cmd(kind, template)` | `src/engine/decls.rs:362` | the `sh` host's own template | `registry.pl:194` | identical | yes |

`created` is the only real miss here: v5 reads `git log --diff-filter=A` per
path for first-author attribution. Nothing in v6 does. Two `.dl` files use it
(`bench/seams/authorship.dl`, `examples/created`-shaped rails), both dead.

### 1.9 engine-internal relations (no extraction content)

28 relations that describe v5's own runtime and die with it. Listed so the
count reconciles; 18 are owed no v6 twin and 10 are simply gone.

| group | rels | why it dies with v5 |
|---|---|---|
| meta (6) | `rel_catalog`, `rel_col`, `fn_catalog`, `op_catalog`, `verb_catalog`, `dl_diag` | v5 describing v5. v6's equivalent is `registry.pl` + `compile/out/manifest.json`, read at compile time |
| diag (4) | `diag`, `diag_stage`, `diag_mute`, `hover_note` | v6 keeps `diag` as an ordinary rel a rail heads (`v6/dl/fixtures/diag-rail.dl6`); `diag_stage`/`diag_mute`/`hover_note` are v5 daemon/LSP wiring |
| graph (2) | `graph_node`, `graph_edge` | the flow panel's drawable sink. No v6 spelling; no v6 flow panel |
| agent (3) | `agent_edit`, `agent_touch`, `skill_loaded` | v5 read harness transcripts. boop owns this now (`~/.agent/boop.db`) |
| perf (2) | `rel_count`, `stmt_ms` | v6 emits `tracing` spans instead (`@engine-tracing-measured`) |
| clock (2) | `every`, `clock` | v6 `bind interval` / the `bucket` freshness column (`registry.pl:359`) |
| daemon (1) | `query_log` | v5 daemon RPC log |
| effect (1) | `effect_log` | v6's `__host_demand`/`__host_response` pair IS the effect log |
| hook (1) | `hook_event` | v5 harness hook ingest |
| embed (1) | `similar` | **missing in v6**: no embedding plane at all, and no `node2vec` |
| propose (2) | `propose_extract`, `propose_clone` | **missing in v6**: no refactor-proposal analysis |
| sys (1) | `env` | `sh env_var` (`registry.pl:464`) |
| type-shape (2) | `type_shape`, `type_lgg` | v5 anti-unification over type shapes. **missing in v6** |
| types (1) | `type_decl_row` | v6 has a real type plane (`compile/0_type_plane.pl`) |

The ten counted `missing` in this group: `similar`, `propose_extract`,
`propose_clone`, `type_shape`, `type_lgg`, `graph_node`, `graph_edge`,
`diag_stage`, `diag_mute`, `hover_note`. The other 18 are `n/a`.

Three of these are genuinely gone with nothing planned: `similar` (embeddings +
`node2vec`), `propose_*` (refactor proposals), `type_shape`/`type_lgg`
(anti-unification). Together they back 6 `.dl` files, all of them
docs-only or orphan in the rail census. Nobody has asked for them since
2026-07-11. They are listed as DELETE, not PORT, in the retirement plan.

## 2. Extraction ops

v5's source ops are the body-position operators that read files.
`op_docs()` at `src/engine/decls.rs:221-254`.

| v5 op | v5 site | what it does | v6 equivalent | v6 site | RECORD | DOOR |
|---|---|---|---|---|---|---|
| `scan(glob, path)` | `decls.rs:224` | select files by glob at a rev | `sh files(glob) -> (path, digest)`, `SoopyFilesExecutor` | `hosts.rs:251-302` | identical | yes |
| `match_line(path, rev, /re/, line)` | `decls.rs:225` | line regex, named captures bind dl vars | `sh ast_rule` with `rule: {regex: ...}`, optionally `all:` with a `kind:` | `1_ast_rule.rs:24` | subset with a NAMED difference: the unit is a grammar node, not a line, so a match cannot straddle nodes and a regex named group does not bind a dl var (the capture set comes from pattern metavariables) | yes |
| `match(...)` | `decls.rs:226` | deprecated alias of `match_line` | same | same | subset, as above | yes |
| `ast(path, rev, :lang, "(query) @cap", line)` | `decls.rs:227` | tree-sitter s-expression query, captures bind | `ts_query/1`, compiles to a `tree_sitter` host demand | `registry.pl:198` | compiles | **no**: `executor_for` has no `tree_sitter` arm (`hosts.rs:41-59`), so the demand has no linked executor |
| `match_ast(path, rev, :lang, "$X.f()", line)` | `decls.rs:228` | ast-grep structural pattern, metavars bind | `sh ast_rule` with `rule: {pattern: ...}`; captures carry `name`/`text`/`span` | `1_ast_rule.rs:20-91`, `registry.pl:336,349` | superset (adds `Kind`, `Not`, `Matches`, `utils`) | yes |
| `sg(...)` | `decls.rs:229` | deprecated alias of `match_ast` | same | same | superset | yes |
| `ast_yaml(path, rev, :lang, "rule yaml", line)` | `decls.rs:230` | ast-grep RuleCore YAML (`inside:`/`has:`) | `sh ast_rule`: `Inside`, `Has`, `Follows`, `Precedes`, each with `stop_by` | `1_ast_rule.rs:28-48` | superset (v5 had `inside:` at the immediate parent only and no `field:`; `stop_by` and the two ordering rules are new) | yes |
| `json(path, rev, q:{ $k: $v })` | `decls.rs:231` | brace pattern over json/yaml/toml, key AND value captures | `record=data_value family=data` + `decode/2` | `schema.rs:43`, `registry.pl:85` | superset (v6 gives every path, not only matched ones) | yes, `--family data` |
| `jsonp(path, rev, "a.*.b", out)` | `decls.rs:232` | dotted path over json/yaml/toml | `record=data_value.path` | `schema.rs:43` | identical | yes |
| `cmd(path, rev, "tool {file}", line, out)` | `decls.rs:233` | shell out per file, one row per stdout line | **deliberately deleted**: "Zero shell in the engine" (CLAUDE.md, 2026-08-21); `ShellExecutor` gone | `hosts.rs:39` `LINKED_EXECUTORS` | **missing by decision** | no |
| `comment(path, rev, /open/[, /close/], l0, l1, label)` | `decls.rs:234` | comment-marker regions, LIFO nesting | `sh ast_rule` with `all: [kind: line_comment, regex: ...]`, and `follows`/`precedes` for the pairing | `1_ast_rule.rs:24,39-48` | subset: the marker regions come out as node spans, and LIFO nesting of a BEGIN/END pair is a dl6 join rather than an op | yes |

Sink ops (not extraction, listed for the retirement plan):

| v5 sink | v5 site | v6 |
|---|---|---|
| `? query` | `decls.rs:248` | `query/1`, `registry.pl:197` — live, with `order by` on the Rust door |
| `diag(...)` | `decls.rs:249` | an ordinary rel a rail heads; `v6/dl/fixtures/diag-rail.dl6` |
| `gen([:mode,] path, [l0, l1,] "template")` | `decls.rs:250` | **missing**: issue `@fs-effects-door` "dl6 writes files", still open. 25 `.dl` files use `gen` |
| `graph_node` / `graph_edge` | `decls.rs:251-252` | **missing**: no v6 drawable-graph sink and no flow panel |
| `hover_note(...)` | `decls.rs:253` | **missing**: `plans/2026-08-12-v6-native-lsp.PLAN.md` |

## 3. Languages

v5's roster is three separate tables that do not agree with each other.

| v5 table | v5 site | languages |
|---|---|---|
| `type_langs()` — type/call/df/doc facts | `src/graph/typegraph/mod.rs:541` | rust, kotlin, ts, go, python (5) |
| modgraph — module edges | `src/graph/modgraph/` | rust, kotlin, ts, go, python (5) |
| `AST_LANG_TABLE` — the `ast` op | `src/engine/lang_tables.rs:4-50` | rust, c, kotlin, python, bash, go, hcl, starlark, jsonnet, dl, gotmpl, dockerfile, yaml, toml, json, css (16 canonical + aliases) |
| `SG_LANGS` — `match_ast` / `ast_yaml` | `src/sg.rs:15-40` | 23 ast-grep grammars |
| `lang_label_for_path` — `comment_node` / cst | `src/cst.rs:226-256` | 16 extensions mapping onto `AST_LANG_TABLE` |

v6's roster is ONE table, `sources()` at `v6/sprefa-extract/src/lang/mod.rs:46-58`.

| v6 `Source` | matches | planes it fills | v5 twin |
|---|---|---|---|
| `RustSource` | `.rs` | cst, type, call, df | `RustTypes` + rust modgraph |
| `GoSource` | `.go` | cst, type, call, df | `GoTypes` + go modgraph |
| `KotlinSource` | `.kt`, `.kts` | cst, type, call, df | `KotlinTypes` + kotlin modgraph |
| `MarkdownSource` | `.md` | cst, `doc_node` | v5 `doc_node`/`doc_ref` |
| `PrologSource` | `.pl` | cst, `reference` | none in v5 |
| `DataSource` | `.json`, `.yaml`, `.toml` | cst, data | v5 `json`/`jsonp` ops |
| `DlSource` | `.dl6` | cst, type, call, specifier | v5 `dl` tree-sitter grammar (cst only) |
| `TsSource` | `.ts`, `.tsx`, `.js`, … | cst, type, call, df | `TsTypes` + ts modgraph |
| `AstgrepSource` | anything `SupportLang::from_path` claims | cst only | v5's `sg` roster |

Per-language parity:

| language | v5 planes | v6 planes | verdict |
|---|---|---|---|
| rust | type, call, df, doc, module, cst | type, call, df, doc, specifier, cst | identical |
| typescript / js | type, call, df, doc, module, cst | type, call, df, doc, specifier, cst, unresolved | superset |
| go | type, call, df, doc, module, cst | type, call, df, doc, specifier, cst | identical |
| kotlin | type, call, df, doc, module, cst | type, call, df, specifier, cst | subset: kotlin has no v5 `doc` oracle and is graded by hand (`tests/19_docs_lang_arms.rs` header) |
| **python** | **type, call, df, doc, module, cst** | **cst only** | **subset, the one real language gap** |
| markdown | cst + `doc_node` | cst + `doc_node` | identical |
| prolog | cst | cst + `reference` | superset |
| dl / dl6 | cst | cst, type, call, specifier | superset |
| the other ~14 ast-grep grammars | cst via `sg`, pattern-queryable | cst via `AstgrepSource`, **not** pattern-queryable from dl6 | subset at the door |

### The python gap

`PythonSource` EXISTS and is exported (`v6/sprefa-extract/src/lang/python/_0_source.rs:460`,
re-exported at `lang/mod.rs:31` and `lib.rs:62`). It fills cst, type and call.
It is **not in `sources()`** (`lang/mod.rs:46-58`), so `source_for("x.py")`
falls through to `AstgrepSource` and a `.py` file yields cst rows and nothing
else. `df` is a written follow-up (`_0_source.rs:16`), and there is no
`tests/fixtures/python/*.v5.jsonl` oracle capture, so nothing grades it against
v5.

Owed: commits D (df) and E (both `Resolve` arms), the roster line, the
`ROSTER_FIXTURES` row in `tests/4_capability_parity.rs`, and a captured v5
oracle. Issue `@extract-python-arm` (open, epic `@extract-port-closeout`).

## 4. SCIP indexers

v5's `INDEXERS` at `src/scip_setup.rs:51-99`, six rows. v6's at
`v6/sprefa-extract/src/scip_ensure.rs:59-100`, six rows, ported verbatim in v5's
order with the marker files, binaries and install hints unchanged.

| lang | marker files | binary | v5 | v6 `ScipSource` |
|---|---|---|---|---|
| rust | `Cargo.toml` | `rust-analyzer` | `scip_setup.rs:53` | `ScipRust`, `scip.rs:120` |
| typescript | `tsconfig.json`, `package.json` | `scip-typescript` | `scip_setup.rs:59` | `ScipTypescript`, `scip.rs:106` |
| python | `pyproject.toml`, `setup.py`, `requirements.txt` | `scip-python` | `scip_setup.rs:66` | `ScipPython`, `scip.rs:167` |
| go | `go.mod` | `scip-go` | `scip_setup.rs:73` | `ScipGo`, `scip.rs:143` |
| kotlin/java | `build.gradle.kts`, `build.gradle`, `pom.xml` | `scip-java` | `scip_setup.rs:80` | `ScipJava`, `scip.rs:181` |
| cpp | `compile_commands.json`, `CMakeLists.txt` | `scip-clang` | `scip_setup.rs:86` | `ScipClang`, `scip.rs:195` |

6/6, identical. `chat_log/20260816.2.…:68` recorded "indexers 3/6"; that is
stale as of `@extract-scip-indexer-roster` (done).

v6 adds what v5 had not: a wall budget with the whole process group killed on
the deadline (`scip_ensure.rs:107-120`, default 600s) and `scip_skip` rows so a
root with no installed indexer exits 0 loudly instead of streaming empty
(`schema.rs:65`, `schema.rs:257-259`).

## 5. Cost notes

Measured, from the sites that carry the numbers.

| what | number | site |
|---|---|---|
| dropping one unread `span` column from a host projection | tick-2 arrivals 21194 -> 3501, run 4.8s -> 3.55s | `v6/dl/deadcode/dead-module-rail.dl6:41-45` |
| per-file `git hash-object` vs one batch over 82 paths | 0.82s -> 0.01s | `v6/dl/deadcode/dead-module-rail.dl6:25-26` |
| re-deriving the repository root per FILE | 2.29s of a 3.55s run | `v6/sprefa-engine-rs/src/hosts.rs:640-646` |
| a host response memo at the extract seam | 5.48s vs 5.33s without it, never hit | `v6/sprefa-engine-rs/src/hosts.rs:631-635` |
| capturing whole JSONL per file instead of one row | 779-file corpus 20.26s -> 62.97s, db 1.0MB -> 595MB | `v6/prolog/ARCH.pl:887` |
| `--scip-facts` full passthrough over v6/tsv2, 204 documents | 177,967 rows / 59.4MB, of which `scip_occurrence` is 123,655 rows / 48.5MB | `v6/sprefa-extract/src/schema.rs:204-206` |
| `--scip-deps` vs madge over v6/tsv2 | recall 0.992, precision 0.988 | `schema.rs:215` |
| `--deps` vs the same oracle | recall 1.000, precision 1.000 (agreement with another syntactic scanner, NOT correctness) | `schema.rs:219-224` |
| a 4th parse of 82 rust files for 0 TS module edges | 0.5s | `v6/dl/deadcode/dead-module-rail.dl6:151-152` |

## 6. Prior art this replaces or extends

| artifact | what it is | status |
|---|---|---|
| `plans/2026-07-30-v5-parity-table.tsv` | 156 rows, DERIVED by `v6/dl/fixtures/v5-parity.dl6` through the tsv2 served engine | **stale and not regenerable**: it scores `gen`, `closure`, `diag` and every built-in rel `absent`, all of which have landed since; and tsv2 is paused (CLAUDE.md 2026-08-21), so `just v5-parity` cannot be re-run |
| `v6/dl/fixtures/v5-parity.dl6` | the program that derived it, five mechanical legs, `# @parity` marker comments | keep as a technique fixture; it is the only program that reads v5's own `op_catalog` |
| `@extract-port-closeout` | 16-row RECORD-seam census | 13 done, 3 open (`@extract-python-arm`, `@extract-module-plane-non-ts`, `@extract-modulef-collapse`) |
| `v6/sprefa-extract/tests/golden_parity.rs` | the captured v5 oracle, 10 `.v5.jsonl` files over ts/go/rust/kotlin | live; no python capture (see the python gap) |
| `v6/sprefa-extract/tests/4_capability_parity.rs` | library-vs-binary reach, compiler-enforced | live; it does NOT cover the dl6 door, which is why this document exists |
| `chat_log/20260816.2.…:68` | "8/10 scip rels, indexers 3/6" | both numbers stale: 10/10 on the wire, 6/6 indexers |

## Every "missing" row, with its issue

| # | v5 thing | v5 site | proposed v6 record / door shape | issue |
|---|---|---|---|---|
| 1 | `match_line`'s LINE unit and its named regex captures | `decls.rs:225` | `ast_rule` answers the rail cases; a regex whose match straddles grammar nodes, and a named group bound as a dl var, are the two things still unsaid | `@dl6-ts-query-executor` |
| 2 | `match_ast` / `sg` / `ast_yaml` / `comment` | `decls.rs:228-234` | **CLOSED 2026-08-21** by `ast_rule` (`3da1100f2`, `cd58a6917`). Exercised end to end by `v6/dl/rails/no-new-eprintln-rail.dl6` | — |
| 3 | `ast` (tree-sitter s-expression) | `decls.rs:227` | a `tree_sitter` arm in `executor_for`; `ts_query/1` already compiles to a host demand | `@dl6-ts-query-executor` |
| 5 | `--family cfg` from dl6 | `hosts.rs:1113` | one `"cfg" => want_cfg = true` arm plus the `flatten_cfg` call; the plane is already tested | `@dl6-cfg-family-unlinked` |
| 6 | `scip_occurrence`, `scip_binding` and the eight `--family scip` rows from dl6 | `hosts.rs:1113` | a `scip.facts.*` host namespace beside the four resolved ones | `@dl6-scip-facts-door` |
| 7 | `module_edge`, `module_unresolved`, `module_binding_resolved`, `crate_edge` from dl6 | `hosts.rs:1071` | `deps` / `package_deps` host names with a project-root input | `@dl6-deps-package-door` |
| 8 | `gen` (codegen sink) | `decls.rs:250` | the write-verb door | `@fs-effects-door` (open) |
| 9 | `graph_node` / `graph_edge` sinks | `decls.rs:251-252` | no v6 flow panel; DELETE unless Chris wants the panel back | — (retirement plan, delete list) |
| 10 | `hover_note` sink | `decls.rs:253` | the v6 native LSP | `plans/2026-08-12-v6-native-lsp.PLAN.md` |
| 11 | `created` (first-author attribution) | `src/rels/git.rs:494` | a soopy `git log --diff-filter=A` host | — (delete list: zero live consumers) |
| 12 | `similar` / `node2vec`, `propose_*`, `type_shape` / `type_lgg` | `rels/embed.rs:29`, `rels/propose.rs:28`, `rels/analysis.rs:364` | no plan and no asker since 2026-07-11 | — (delete list) |
| 13 | python type/call/df/module | `graph/typegraph/python.rs` | commits D+E of the go-shaped arm, plus the roster line | `@extract-python-arm` (open) |
