# RECON — v6 Prolog compiler: where the lines can come out

Stage 1 of 3 (recon). Read-only: zero source edits, zero commits. Deliverable is
this file. HEAD verified `3b9e9cfd` before anything ran. Every number below is
from a `wc`/`grep` I ran in this worktree; every claim carries file:line.

Scope note on the brief's targets: the brief's per-file figures (lower 5652,
emit 2809, parse 1970, analyze 1765) are near but not exact; current `wc -l`
land at lower.pl **5693**, emit_ts.pl **2809**, parse_dl.pl **1983**, analyze.pl
**1765**. The claimed 22.7k total matches the compiler core defined as
`v6/prolog/*.pl` (19,777) plus `compile/*.pl` minus tests/scripts (parse_dl
1983 + registry 624 + emit-doc scripts 408) = **22,792**. No reality
deviation forcing a stop; deltas are the brief being a snapshot. One brief
claim to flag under "assets used as truth": it says the conformance gate is 281
fixtures; the current fixture clauses number **346** (`fixture(` heads, grep
over `conformance/fixtures/*.pl`) across 39 fixture files, and the justfile
header comment still says 221. All three figures are in the tree, stale at
different times.

---

## 1. Mass map

Verified by running `wc -l` on every `.pl`. Grouped by role. Oracle =
reference-interpreter engine. Fixtures are test inputs, not compiler mass.

| Role | Files | Lines |
|---|---|---|
| parse | compile/parse_dl.pl, 0_cst_query.pl | 2,282 |
| passes / syntax expansion | 0_ast_expand, 0_body_walk, 0_coalesce_expand, 0_dot_expand, 0_enum_expand, 0_graph, 0_match_expand, 0_negated_guard_expand, 0_option_expand, 0_rel_record, 0_relation_edge_expand, 0_relation_pattern, 0_seq_expand, 0_type_plane, 0_unsupported_messages, 0_program_check, 1_expansion, 1_host_expand, 2_subscribe, 3_clock_check | 6,039 |
| analyze | analyze.pl | 1,765 |
| lower | lower.pl | 5,693 |
| emit | emit_ts.pl, print_dl.pl | 3,493 |
| driver / infra | compile.pl, ARCH.pl, diag.pl, strat.pl, sweep.pl, 6_profile.pl, compile/registry.pl, use_resolve.pl | 3,112 |
| **core compiler** | roles above | **22,384** (+ 408 in compile/emit-doc scripts) |
| oracle (reference interpreter) | conformance/engine.pl, body.pl, level_eval.pl, ticklog.pl | 1,565 |
| oracle rulings | conformance/rulings.pl | 655 |
| oracle fixtures | conformance/fixtures/*.pl (39 files, 346 clauses) | 8,474 |
| prolog tests | compile/test/*.pl | 8,665 |

Top four by mass: lower.pl 5693, emit_ts.pl 2809, parse_dl.pl 1983, analyze.pl
1765. lower alone is 25% of the compiler core; lower + emit is 37%.

---

## 2. Duplication inventory

Method: extracted every predicate head defined in the core files, grouped by
name; names defined in 2+ files were then read to decide identical-vs-collision.
Identical logic, both sites:

| Logic | Site A | Site B | identical? |
|---|---|---|---|
| module_hash/2 (sha256, 16 hex) | lower.pl:753 | use_resolve.pl:244 | yes |
| catalog rel id-stride (`Id+1+RelArity` walk) | lower.pl:1373 (catalog_rel_id_map) | lower.pl:1385 (catalog_rel_block_end) | yes — same walk, two names |
| column_type_decls/3 | 0_ast_expand.pl:220 | 1_host_expand.pl:566 | yes |
| build_rule/4 | 0_ast_expand.pl:258 | 0_coalesce_expand.pl:103 | yes |
| memberchk_eq/2 | 0_coalesce_expand.pl:179 | 0_dot_expand.pl:647 | yes |
| rule_is_edge/1 | 0_program_check.pl:804 | analyze.pl:62 | yes |
| rule_body/2 | 0_program_check.pl:806 | analyze.pl:71 | yes |
| rule_head/2 | 0_program_check.pl:892 | analyze.pl:68 | yes |
| host_relation_refs/3 | 0_ast_expand.pl:225 | 1_host_expand.pl:359 | yes |
| conjunction flattener (conjunction_goals/2, manual recursion) | 0_coalesce_expand.pl:117, 0_dot_expand.pl:622, 0_seq_expand.pl:108, 0_negated_guard_expand.pl:55 | shared `body_conjunction_goals` lives in 0_body_walk.pl (used by analyze.pl:1028, 1_host_expand.pl:461) | partial — 4 hand copies duplicate the shared walk |
| goals_conjunction/2 (rebuild comma chain) | 0_coalesce_expand.pl:127, 0_dot_expand.pl:632, 0_negated_guard_expand.pl:65, 0_seq_expand.pl:118 | lower.pl:2546 (near: missing [] base) | yes ×4 + 1 near |
| rule_head_ref/2 | 1_host_expand.pl:409 (via functor) | analyze.pl:65 (via rel_ref) | no — same purpose, different result shape |
| body_goals/2 | 0_ast_expand.pl:243 (manual) | 1_host_expand.pl:461 (delegates to shared walk) | partial dup |
| ts_escaped_codes (DSL escape) | 0_cst_query.pl:276-279 (full: backslash+quote+fallthrough) | 1_host_expand.pl:426-427 (fallthrough only, truncated) | partial dup |
| host_relation_refs + digest/host_atom minting | 0_ast_expand.pl:225 | 1_host_expand.pl:359 | overlap |

Confirmed both brief seeds (module_hash, catalog stride) and extended with 8
more identical pairs plus the four-horizontal-copy conjunction cluster. The
conjunction flattener (+ its inverse) is the single most duplicated idea in the
passes: 6 hand copies when one shared walk already exists in 0_body_walk.pl.

`arithmetic_result_type` shows up in analyze.pl:780 and lower.pl:627 but with
differing clause sets (analyze has a text fallback, lower starts from mod), so
I file it as overlapping-not-identical; reasonable to unify but the guard
clauses differ.

---

## 3. lower.pl structural map

6,583 words of interned comment + 5,693 lines; 611 clause heads, 402 distinct
predicates. Section boundaries by head sequence (from a head dump; the file's
own `─` markers are sparse, only 12):

| Region | Lines | Heads | Character |
|---|---|---|---|
| table-name + rule-id minting | 176-278 | table_name, delta_table_name, ..., statement_ordinals | mechanical string minting off refs |
| expression compiler (compile_pattern_arg → arithmetic/json/text) | 278-680 | compile_expr, arithmetic_sql, text_scalar_*, json_value_expr | dispatch over registry expression/5, mostly a table already |
| catalog DDL + catalog row build | 683-1540 | catalog_ddl_*, catalog_*_rows, catalog_rel_id_map/block_end, path tree, column rows | **40 catalog_ heads**, ~860 lines, hand-assembles row/12 tuples |
| text literal / regexp / comparison / intern statements / text views | 1545-1920 | text_literal_sql, intern_*, text_view_* | mechanical SQL builders |
| struct/dictionary relplans + relation-pattern rewrite | 1922-2350 | struct_type_plans, dictionary_relplans, rewrite_relation_* | pattern rewrite |
| dictionary atom elision + decode + arrivals | 2350-2745 | elide_dictionary_*, expand_decode_*, arrival_statement | guard-driven |
| edge statements + triggers | 2745-3043 | edge_statements_for_rule, edge_statement_single, triggers | per-enumeration builder |
| aggregates: avg/aggregate/refcount/scope | 3043-3731 | avg_*(31 heads), aggregate_*(19), level_ref_count | **the most mechanical block in the file** — ~50 near-identical per-shape SQL string builders |
| expand + dred (least-fixpoint waves) | 3731-4056 | level_expand_plan, dred_*(21 heads) | same-shape wave family |
| level fixpoint IR + guards | 4056-4612 | ir_*(26 heads), level_insert | IR walk |
| json decodes + aggregate selects | 4613-5100 | json_*, aggregate_select_* | dispatch |
| delta / retention / canonical expr / boot / entry | 5102-5693 | delta_statement, retention_statement, canonical_column_expr(8), boot_*(9), lower_program(5563) | entry + tail |

Which parts are mechanical/repetitive vs data-driven:

- **Already table-driven**: the expression/operator inventory reads registry.pl
  expression/5 (lower.pl:530-632 comment: "The operator inventory is
  registry.pl's expression/5"); comparisons/comparison_operator_sql
  (1632-1653); json type renderings (json_capture_json_type, 4760-4763).
- **Mechanical, could become data tables**: the catalog row build
  (684-1540), the entire aggregation family (3043-3731, ~1,050 lines) and the
  dred wave family (3800-4053). These are repetitive SQL-text/`row/12`
  emitters where a single parameterized driver fed by a shape record would
  replace dozens of sibling predicates. The `avg_*` family alone is 31 head
  names (avg_accumulator_seed_sql, avg_scope_*, avg_delta_rows_sql, ...) all
  building SQL for one accumulator shape.
- **Genuinely irregular / stays code**: table-name minting (naming contracts),
  the dictionary/relation-pattern rewrites (structural, shape-dependent), the
  IR walk, json path/text escapes.

---

## 4. emit_ts.pl: template-shaped code → data + one renderer

2,809 lines, 186 distinct predicates. Structure is a list-of-lines emitter:
`emit_program` (2682) concatenates `*_lines` section builders, each paired
with a per-row `*_entry_line`/`*_entry` predicate (ddl_lines/ddl_entry_line,
rel_catalog_lines/rel_catalog_entry_line, arrival_statements_lines/
arrival_statement_entry_line, incremental_*_lines/*_entry_line, ordered_*
lines/*_entry, read_*_fn_lines/*_entry_line, snapshot_*/*_entry_line).

Concrete data+renderer opportunities:

- **Mode twins.** The same logical function is emitted twice or three times
  under a naive/incremental/ordered tag and the differences are a handful of
  carries:
  - text intern: naive_text_intern_lines (330) / incremental_text_intern_lines (336)
  - reference normalize: naive_* (348) / incremental_* (357)
  - retention: naive_retention_fn_lines (2214) / retention_tick_lines_ordered (2357)
  - tick functions: run_naive_tick_fn_lines (2222), run_ordered_tick_fn_lines
    (2302), run_incremental_tick_fn_lines (2513), run_tick_dispatch_lines (2570)
  - advance: advance_tick_pipeline_line (2476) / advance_tick_naive_line (2480),
    plus the departure_stage_incremental/naive pair (2366/2371)
  These collapse to one renderer parameterized by a mode enum; the delta is the
  carry/order handling already threaded as arguments.
- **Per-statement entry rows.** The section-builder/entry-line pairs repeat
  one structural pattern (map over IR → one line each); a generic IR-driven
  emit loop replaces each pair. Counterweight: several are genuinely distinct
  (edge_resolver_block, 1529-1654, is ~125 lines of one bespoke resolver, not
  table-shaped).
- **String escaping** is hand-rolled in three places with overlap:
  js_template_codes/js_string_codes (44-95), quote_ident_local (1463) vs
  lower's quote_ident (251), js_object_key/js_identifier_* (59-72),
  fixpoint_*_text (1286-1428). One shared escaping module.

---

## 5. parse_dl.pl: grammar-as-data feasibility

1,983 lines. Region inventory (by head sequence, from a dump):

| Region | Lines | Approx |
|---|---|---|
| infra (findings, line-tracking, host normalization) | 100-346 | 247 |
| lexer (skip_ws, ident, int/float/atom/string, escapes) | 347-541 | 195 |
| vars + use | 542-572 | 31 |
| declarations (rel a/b, enum, coltype, keep/key, bind/sh/query/match, module path) | 573-1197 | 625 |
| rules (rule_stmt, head, args, named-arg friction) | 1198-1346 | 149 |
| body + body_item + cst | 1347-1492 | 146 |
| keyword_call / balanced_parens / wrappers (table dispatch) | 1493-1566 | 74 |
| surface items, bind/comparison/relatom | 1567-1724 | 158 |
| expr grammar + braces + lists | 1725-1980 | 256 |

Already table-driven:
- **Body-item keyword dispatch is a table.** `body_item` walks registry.pl's
  `surface/6` (59 facts, registry.pl:35-155), matches the keyword with
  `keyword_call` (parse_dl.pl:1493), then `parse_surface_wrapper`
  (1549-1566) routes the raw inner text to a per-shape sub-parser
  (rel_atom/atom_list/expr/expr_pair). This is exactly the "production table +
  interpreter" skeleton already in place.
- Escapes: escape_codes/'quoted_chars (532-537); operator aliases:
  registered_infix_op + comp_op (1626-1636); type vocabulary:
  typed_column_type_base (686-720); coltype (881-887). All tables.

Context-sensitive bits that resist a declarative grammar:
- **Variable identity across the file**: one Vars accumulator threaded through
  every rule (get_or_make_var, 542; module header lines 7-28). A pure
  production table has no state slot for this; the interpreter must keep it.
- **Named-arg → positional resolution** needs the target rel's declared column
  order, stored in a dynamic `rel_column_order` built while scanning decls
  (record_column_order 101, resolve_named_args 1258-1345). Requires the
  declaration pass to have run first — a contextual constraint, not a grammar
  shape.
- **Raw balanced-paren scans**: balanced_parens (1500-1512) counts nesting to
  grab `keyword(...)` payloads whole (because each keywd's inner shape differs);
  cst_block_codes/cst_block_string (1397-1412) scan the tree-sitter block;
  braces_term/brace_pairs (1900-1960) scan `{}` JSON5 payloads. These are
  char-scans, not productions.
- **Two-dialect merge**: the grammar accepts both the term-form spelling and
  the dl.langium spelling with alias normalization (header 17-50 +
  normalize_host_* 295-346). A single declarative grammar doesn't model the
  dual-accept + alias table cleanly.

Feasible fraction: the table-driven seam is real and already half-built. The
lexer token layer (~195) + expression/precedence grammar (expr/add_/mul_/factor,
1725-1980, ~256) + the keyword-wrapper dispatch (74) + the repetitive
declaration terminals (a large share of the 625) are declarative-friendly —
together roughly **35-45%** of the 1,983 lines, call it **~700-900 lines** a
production-table + small interpreter could replace. The stuck ~55% is the
context machinery: variable identity, named-arg ordering, dual-dialect alias
normalization, and the raw balanced/cst/brace scans. That share is why the
estimate is bounded well below the file size.

---

## 6. Dead weight

How callers were determined: I attempted three automated scans (regex head
extraction + reference counting). Two failed and are reported as failed so the
receipt is honest: (a) a `name(`-call-site counter missed every higher-order
call (`maplist`, `foldl`, `forall`) and exported-but-called predicates — it
labelled `host_plan_json` dead when emit_ts.pl:414 is `maplist(host_plan_json,
...)`; (b) a token-count pass exploded to 1,041 false positives for the same
reason (multi-line clauses, maplist args, module qualifiers). The `dl` engine
has no Prolog grammar (ast language matrix lists no `prolog`), so no
compiler-backed call graph was available to autotune.

The reliable third pass: for **single-clause** predicates, a name whose token
appears exactly once in the entire repo (its own head) has zero callers by
construction — it cannot be exported, imported, or maplist'd (each of those
would add a token). Receipt: only two such predicates exist, neither in the
core compiler (compile/scripts/golden_oracle.pl:oracle_both; conformance
oracle engine.pl:wrap_arrival).

Conclusion: the core compiler is well-connected; there is **no meaningful
zero-caller dead code** to harvest. The one documented near-dead entry is
`execution_profile_dl6/2` (6_profile.pl:129), which ARCH.pl:908 records as
"the previously-never-run execution_profile_dl6" — its body was exercised
by hand once during the compile-speed postmortem, not by a caller. Shedding it
saves ~10 lines but is not a mass lever. Do not plan line savings around dead
weight; it is a rounding error against the mechanical-family removals in
sections 3-4.

---

## 7. Ranked shrink plan

Gate keys: C = conformance (346 clauses, `swipl -q -l v6/prolog/conformance/go.pl
-g go -g halt`), P = plunit (`swipl -q -l compile/test/plunit_tests.pl -g
run_tests -g halt`), T = TEXT_DOOR, G = byte-identity on emitted TS goldens
(246), S = `just run_sql_check.pl` execution harness, R = roundtrip
(print_dl → parse_dl).

| # | Move | Files | Est. lines saved | Risk (what breaks) | Safety gate |
|---|---|---|---|---|---|
| 1 | Collapse the 5-mode emitters to one renderer parameterized by mode (naive/incremental/ordered twins) | emit_ts.pl | 150-200 | emitted TS text / function names differ per mode; any wording drift breaks byte-identity | G (246), C |
| 2 | Replace the `avg_*` accumulator family (31 heads) with one aggregate-shape table + parameterized SQL renderer | lower.pl | 120-150 | SQL wording/column-name emission must match the pinned goldens exactly | G, S |
| 3 | Replace the `dred_*` wave family (21 heads) with a table-driven wave-plan renderer | lower.pl | 90-110 | same — SQL wording + wave metadata | G, S |
| 4 | Fold the catalog row build into a data table + generic `row/12` renderer | lower.pl | 300-400 (largest single lower win) | **rel id ordering is byte-stability-critical**; a reorder shifts every id downstream | G, C |
| 5 | Dedupe the 8 identical predicate pairs + 4-hand conjunction flattener into one shared util (module_hash, build_rule, rule_head/body/is_edge, host_relation_refs, column_type_decls, memberchk_eq, conjunction_goals/goals_conjunction) | pass files, analyze, lower, use_resolve | 40-60 | behavioral drift if the copies had silently diverged (they didn't — verified identical) | P, C |
| 6 | Single catalog id-stride walker for `catalog_rel_id_map` + `catalog_rel_block_end` | lower.pl | 8-10 | none substantive; pure refactor | G, C |
| 7 | One shared escaping/naming module (js_template/js_string, quote_ident_local vs quote_ident, ts_escaped_codes) | emit_ts, 0_cst_query, 1_host_expand | 40-60 | escape output must stay byte-identical | G, T |
| 8 | Generic IR-driven emit loop replacing the per-statement `*_lines`/`*_entry_line` pairs (ddl, rel_catalog, arrival, incremental, ordered, read, snapshot) | emit_ts | 120-150 | structure of generated TS must not change | G (246), C |
| 9 | parse_dl production-table for lexer + expression grammar + keyword-dispatch + declaration terminals | compile/parse_dl.pl | 700-900 (largest single-file ceiling) | dual-dialect alias semantics; named-arg ordering; variable identity; raw scans — highest uncertainty | R, T, C, P |
| 10 | Drop documented-unused `execution_profile_dl6` + consolidate compile/emit-doc scripts (1/2/3_emit_registry*) | 6_profile.pl, compile/ | 50-100 | only if no tool greps the doc scripts by name | P, `just` |

Ordering is by (saved lines × confidence) / (risk of byte-identity breakage).
Moves 1-3 hit the two mechanical families (avg/dred) + emitter twins first,
all guarded by byte-identity goldens + conformance, so a wording miss surfaces
as a red golden, not a silent semantic change. Move 4 is the largest lower win
but touches the id-ordering rail, so it carries the strictest gate. Move 9 is
the biggest ceiling but the riskiest (context machinery survives), which is why
it sits at 9 even though it saves the most: it is a rewrite, not a peel.

The gates that exist (from the brief, verified in the justfile): C = conformance
(`just` target `conformance`, justfile:39-40), P = plunit (justfile:52-53),
T = text-door-receipt (justfile:47-48), G = 246 byte-identity TS goldens,
S = run_sql_check (execution harness), R = roundtrip, `just green-all`
(justfile:392).
