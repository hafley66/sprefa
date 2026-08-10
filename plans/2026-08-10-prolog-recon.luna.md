# Prolog compiler shrink reconnaissance

HEAD receipt: `git rev-parse --short=8 HEAD` printed `3b9e9cfd`.

## 1. Mass map

Measured with `wc -l` in this worktree. The brief's `lower.pl` and
`parse_dl.pl` headline counts are stale relative to these files.

| Role | File | Lines |
|---|---|---:|
| parse | `v6/prolog/compile/parse_dl.pl` | 1,983 |
| parse | `v6/prolog/print_dl.pl` | 684 |
| passes | `v6/prolog/0_ast_expand.pl` | 259 |
| passes | `v6/prolog/0_body_walk.pl` | 212 |
| passes | `v6/prolog/0_coalesce_expand.pl` | 274 |
| passes | `v6/prolog/0_cst_query.pl` | 299 |
| passes | `v6/prolog/0_dot_expand.pl` | 648 |
| passes | `v6/prolog/0_enum_expand.pl` | 199 |
| passes | `v6/prolog/0_graph.pl` | 196 |
| passes | `v6/prolog/0_match_expand.pl` | 137 |
| passes | `v6/prolog/0_negated_guard_expand.pl` | 68 |
| passes | `v6/prolog/0_option_expand.pl` | 122 |
| passes | `v6/prolog/0_program_check.pl` | 940 |
| passes | `v6/prolog/0_rel_record.pl` | 104 |
| passes | `v6/prolog/0_relation_edge_expand.pl` | 92 |
| passes | `v6/prolog/0_relation_pattern.pl` | 102 |
| passes | `v6/prolog/0_seq_expand.pl` | 194 |
| passes | `v6/prolog/0_type_plane.pl` | 891 |
| passes | `v6/prolog/0_unsupported_messages.pl` | 237 |
| passes | `v6/prolog/1_expansion.pl` | 98 |
| passes | `v6/prolog/1_host_expand.pl` | 611 |
| passes | `v6/prolog/2_subscribe.pl` | 92 |
| passes | `v6/prolog/3_clock_check.pl` | 563 |
| passes | `v6/prolog/6_profile.pl` | 139 |
| analyze | `v6/prolog/analyze.pl` | 1,765 |
| lower | `v6/prolog/lower.pl` | 5,693 |
| emit | `v6/prolog/emit_ts.pl` | 2,809 |
| oracle | `v6/prolog/conformance/engine.pl` | 668 |
| oracle | `v6/prolog/conformance/body.pl` | 362 |
| oracle | `v6/prolog/conformance/level_eval.pl` | 332 |
| oracle | `v6/prolog/conformance/rulings.pl` | 655 |
| oracle | `v6/prolog/conformance/ticklog.pl` | 203 |
| oracle | `v6/prolog/conformance/go.pl` | 27 |
| oracle | `v6/prolog/conformance/fixtures/*.pl` | 39 files; 8,447 total |
| infra | `v6/prolog/compile.pl` | 571 |
| infra | `v6/prolog/compile/registry.pl` | 624 |
| infra | `v6/prolog/use_resolve.pl` | 259 |
| infra | `v6/prolog/diag.pl` | 187 |
| infra | `v6/prolog/strat.pl` | 114 |
| infra | `v6/prolog/sweep.pl` | 230 |
| infra | `v6/prolog/ARCH.pl` | 988 |
| infra | `v6/prolog/compile/test/plunit_tests.pl` | 7,283 |

The grouped `wc -l` receipts were 19,777 lines for the selected compiler
source set, 10,694 for `conformance/*.pl` plus its fixtures, and 12,813 for
`compile/*.pl`, `compile/scripts/*.pl`, and `compile/test/*.pl`.

The language manifest is `v6/prolog/compile/out/manifest.json`; `jq` reports
346 top-level entries. The output directory contains 246 `*.schedule.json`
goldens by `find`.

## 2. Duplication inventory

| Repeated logic | Site 1 | Site 2 | Receipt |
|---|---|---|---|
| Module hash calculation | `v6/prolog/use_resolve.pl:244` | `v6/prolog/lower.pl:753` | Both define `module_hash/2`; callers include `use_resolve.pl:108` and `lower.pl:1253`. |
| Relation catalog ID stride | `v6/prolog/lower.pl:1374-1377` | `v6/prolog/lower.pl:1386-1389` | Both add `1 + RelArity` while walking `RelPlans`; the two predicates are `catalog_rel_id_map/4` and `catalog_rel_block_end/3`. |
| Primitive type row construction | `v6/prolog/lower.pl:1315-1321` | `v6/prolog/lower.pl:1355-1362` | Both are positional row walkers carrying an ID accumulator, for primitive and list rows. |
| SQL template array rendering | `v6/prolog/emit_ts.pl:1127-1130` | `v6/prolog/emit_ts.pl:1278-1281` | Both map `js_template/2`, join with commas, and format `[~w]`. |
| Optional SQL-to-template conversion | `v6/prolog/emit_ts.pl:1211-1212` | `v6/prolog/emit_ts.pl:1232-1244` | `optional_sql_template/2` and `expand_sql_text/2` repeat `none -> null` plus `js_template/2` over SQL fields. |
| Snapshot read entry rendering | `v6/prolog/emit_ts.pl:901-905` | `v6/prolog/emit_ts.pl:928-932` | Delta and stored-delta entries build the same `select_rows` line from a relation, SQL template, columns, and types. |
| Column-list declaration parsing | `v6/prolog/compile/parse_dl.pl:665-668` | `v6/prolog/compile/parse_dl.pl:854-858` | `decl_a_columns/3` and `decl_b_columns/4` repeat comma-separated column recursion and whitespace handling. |
| Declaration reference renaming | `v6/prolog/compile/parse_dl.pl:915-922` | `v6/prolog/compile/parse_dl.pl:936-940` | Module-path collision handling repeats exhaustive declaration-kind traversal over `Decls`. |
| Trigger-kind normalization | `v6/prolog/lower.pl:2805-2807` | `v6/prolog/emit_ts.pl:1766-1767` | Both define a default mapping and a special `ordered_departure` mapping for trigger kinds. |
| JSON capture type mapping | `v6/prolog/lower.pl:4754-4778` | `v6/prolog/conformance/body.pl:237-240` | Lowering comments identify the clause-for-clause mirror; both map int, float, text, and unknown types. |

The last row is a cross-file inventory item. The exact receipt command was:
`rg -n 'json_capture_type' v6/prolog/lower.pl v6/prolog/conformance/body.pl`.

## 3. `lower.pl` structural map

| Lines | Section and contents | Table-shaped candidates |
|---:|---|---|
| 174-251 | Identifier, table-name, SQL-literal helpers | Small name and literal tables interpreted by one formatter. |
| 266-650 | Pattern arguments, expression compilation, text, JSON, arithmetic, concat | Operator and scalar-function rows already exist in `compile/registry.pl`; dispatch and type checks remain driver logic. |
| 651-672 | Guard and bind goals, tick table | Goal-kind rows for guard/bind SQL fragments. |
| 680-1164 | Catalog DDL, hashes, catalog planes, level-plane families | Catalog row-family data can describe row kind, name template, parent, arity, and ID stride. The family-specific predicates at `895-1161` are the largest mechanical block. |
| 1168-1538 | Ports, storage, declaration rows, list rows, relation rows, column rows | Row descriptors plus a single positional walker can cover `catalog_primitive_rows/2`, `catalog_list_rows/5`, `catalog_rel_rows/8`, and `catalog_column_rows/9`. |
| 1545-1805 | Guard lowering, regexp, comparison, interning, literal seeds | Operator, comparison, and encoding tables. |
| 1822-2149 | Text decode views, intern plans, DDL, relation rendering | DDL column and rendering descriptors. `column_def/3` has multiple clauses at `1985-2023`. |
| 2153-2469 | Relation-reference dictionary joins and pattern rewrites | Pattern-kind and rewrite-action rows; recursive relation-value traversal remains a driver. |
| 2496-2709 | JSON decode lowering and arrival SQL | JSON pattern operation rows, with path accumulation and binding state in the driver. |
| 2714-3019 | Edge-rule lowering and trigger projection | Trigger classification and write-shape tables; SQL projection still uses bound variables and rule-specific joins. |
| 3021-3789 | Level rules, grouping, averages, recursive maintenance | Aggregate and statement-family descriptors; SQL fragments can be stored as templates while cardinality and binding decisions stay procedural. |
| 3790-4051 | DRED recursive-head maintenance | DRED family descriptors for ping, pong, cone, probes, and commit statements. |
| 4052-4838 | Backend-neutral fixpoint IR, decode, JSON pattern lowering | Fixpoint IR is already a term data model; renderer/validator separation is a candidate. |
| 4839-5693 | Aggregate heads, ordered aggregates, final lowering assembly | Aggregate-kind and argument-position tables can replace repeated clause families; order-sensitive SQL assembly stays in a small driver. |

The mechanical areas are the repeated row constructors, repeated ID
increments, repeated `row(...)` field placement, repeated SQL fragment
assembly, and repeated operator/type dispatch. The semantic areas are
variable binding, relation-path resolution, JSON path state, recursive DRED
state, aggregate grouping, and unsupported-construct checks.

## 4. `emit_ts.pl`

Template-shaped code is concentrated in `maplist` plus `format` pairs:

- `js_template/2` and `js_string/2` perform escaping at `37-96`.
- Array constants use `array_const_line/3` at `423-443`.
- DDL, relation catalog, declared types, boot rows, snapshots, final selects,
  arrivals, and statement records each have an entry-line predicate at
  `732-1,008`.
- Incremental edge, level, and retention records are assembled by entry-line
  predicates at `1,103-1,211`.
- SQL arrays and fixpoint IR use repeated `maplist` plus `format` renderers at
  `1,214-1,436`.
- Ordered and naive runtime function bodies remain hand-assembled in the
  ranges `2,036-2,570`.

A data representation can cover each repeated output record: field name,
source term projection, optionality, scalar renderer, array renderer, and
line template. One renderer can then handle the entry-line families. The
ordered runtime bodies contain control-flow placement and should remain
separate from record serialization.

## 5. `parse_dl.pl` grammar-as-data feasibility

Measured file size: 1,983 lines from `wc -l`.

| Production family | Sites | Data-table status |
|---|---|---|
| Top-level statement alternatives | `573-581` | Ordered dispatcher over declaration, query, match, and rule handlers. |
| A-style declarations | `598-663` | Partially table-shaped by declaration kind and modifiers. |
| B-style declarations | `819-887` | Repeats column and type handling from A-style declarations. |
| Enums | `722-778` | Variant and field recursion can use production descriptors. |
| Bind and host declarations | `1023-1112` | Column/type loops can use the same descriptor as relation declarations. |
| Queries, matches, rules | `1129-1219` | Surface productions are explicit and carry variable-state threading. |
| Body | `1347-1510` | Keyword calls are already table-driven through the list beginning at `1362`; generic body alternatives remain ordered. |
| Expressions | `1719-1790` | Precedence layers are regular and can be described by operator tables. |
| Braces / JSON-shaped values | `1880-1964` | Key, hole, spread, capture, type marker, and object recursion carry semantic state. |

The keyword-call list is already the clearest data-table seam: the dispatch
starts at `1362`, calls `keyword_call/4` at `1372`, and consumes balanced raw
contents through `1493-1510`. A production-table interpreter can replace the
repetitive delimiter, keyword, comma-list, and precedence boilerplate.

Context-sensitive pieces that resist a declarative production table include
the balanced-parenthesis raw scan at `1487-1510`, because it preserves nested
raw codes before a second parser runs; template literals at `1114-1124`,
because backtick and backslash handling have a separate escape contract;
variable identity threading through `get_or_make_var/4` at `542-550`; named
argument placement at `1248-1341`; and JSON5-subset brace semantics at
`1884-1964`, including `$` holes, `**` spread, captures, typed captures, and
the no-trailing-comma rule.

The regular production surface is approximately 1,150 of 1,983 lines when
counting top-level statements, declaration/list loops, keyword dispatch,
and precedence expressions at `347-488`, `573-887`, `1023-1112`, `1129-1219`,
`1347-1510`, and `1719-1790`. A production-table interpreter could target
approximately 600-800 source lines saved after retaining diagnostics,
variable threading, raw scans, and JSON handlers. This is an estimate from
the measured production ranges, not a compiled delta.

## 6. Dead weight

Method: predicate heads were extracted with an `awk` definition scan from
`v6/prolog/lower.pl`; each name was searched with `rg -o -w` across
`v6/prolog --glob '*.pl'`; candidates with one occurrence or fewer were
reported. The receipt returned no candidates. Therefore this pass found zero
lowering predicates with zero textual callers. This is a textual caller scan;
it does not prove runtime reachability or account for meta-calls.

The exact command was:
`awk '/^[a-zA-Z_][a-zA-Z0-9_]*\\([^%]*\\) *(:-|-->|:-)/ ...' v6/prolog/lower.pl`
followed by `rg -o -w NAME v6/prolog --glob '*.pl'` for each extracted name.

## 7. Ranked shrink plan

Estimated savings are source-line estimates tied to the cited blocks. Safety
gates are existing project gates named in `v6/prolog/compile/scripts/roundtrip.sh:1-11`,
`v6/prolog/compile/scripts/text_door_receipt.sh:1-7`, and
`v6/prolog/labs/diag_channel/CONTRACT.md:99-101`.

| Rank | Move | Files | Estimated lines saved | Risk | Gate |
|---:|---|---|---:|---|---|
| 1 | Replace repeated catalog row-family constructors with descriptor tables and one ID/row walker | `lower.pl:878-1236` | 700-1,000 | Catalog IDs, parent IDs, or emitted row order change | Conformance 281/0; byte-identity on 246 TS goldens; `just green-all` |
| 2 | Unify declaration and column-list grammar loops | `parse_dl.pl:598-887` | 180-260 | A/B surface precedence or diagnostics change | TEXT_DOOR 196/196; plunit; conformance |
| 3 | Generate emitter record serializers from field descriptors | `emit_ts.pl:732-1,436` | 250-400 | Property names, null handling, or line bytes change | Byte-identity on 246 TS goldens; TEXT_DOOR 196 |
| 4 | Factor relation ID stride and positional row walks | `lower.pl:1315-1509` | 100-170 | ID map and catalog block end diverge | Manifest diff plus byte-identity; conformance |
| 5 | Move expression/operator/type rendering to registry-backed tables | `lower.pl:478-648`, `compile/registry.pl:1-624` | 160-260 | Numeric coercion or SQL operator spelling changes | plunit; conformance; byte-identity |
| 6 | Factor repeated emitter SQL-template arrays and optional fields | `emit_ts.pl:1,211-1,436` | 90-150 | `none -> null` and template escaping changes | 246 TS goldens; TEXT_DOOR |
| 7 | Table-drive JSON pattern operations while retaining a stateful path driver | `lower.pl:2496-2709`, `parse_dl.pl:1880-1964` | 180-300 | JSON spread, capture, or descent row cardinality changes | Conformance; TEXT_DOOR; plunit |
| 8 | Factor trigger-kind and edge-write descriptors | `lower.pl:2714-3019`, `emit_ts.pl:1,490-1,847` | 120-220 | Arrival order and keyed-write semantics change | Conformance; byte-identity; `just green-all` |
| 9 | Convert regular parser precedence and delimiter productions to a production table | `parse_dl.pl:347-488`, `parse_dl.pl:1347-1790` | 300-500 | Furthest-error locations and variable scope change | TEXT_DOOR 196/196; plunit; conformance |
| 10 | Represent fixpoint IR serialization as field and variant tables | `emit_ts.pl:1,285-1,436`, `lower.pl:4052-4230` | 120-200 | Runtime IR shape or optional probe fields change | Byte-identity; plunit; `just green-all` |

Existing gate receipts found in the tree include the 196-fixture TEXT_DOOR
contract at `v6/prolog/compile/scripts/text_door_receipt.sh:7`, the 246 output
goldens measured in `v6/prolog/compile/out`, and the documented battery
`conformance 281/0`, `plunit 276`, and `TEXT_DOOR 196/196/0` at
`v6/prolog/labs/diag_channel/CONTRACT.md:101`.
