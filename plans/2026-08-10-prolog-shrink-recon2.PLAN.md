# Prolog shrink recon, measured second pass

## Context

This pass is read-only with respect to compiler behavior and starts at
`b3c2b711`. The current core is 23,111 lines; its largest files are
`lower.pl` at 5,688 lines, `emit_ts.pl` at 2,799, `parse_dl.pl` at 2,019,
`analyze.pl` at 1,765, and `0_program_check.pl` at 940. The source locations
also expose the relevant ownership boundaries: JSON lowering begins at
`lower.pl:4554`, emitter plan serialization at `emit_ts.pl:1033`, statement
dispatch at `parse_dl.pl:582`, and shared invalid-program checks in
`0_program_check.pl:1-16`.

The calibration replaces the stage-1 estimates. Dead-tree removal produced
25,111 deleted lines. The catalog descriptor pilot took the profitable slice.
The two later descriptor conversions on `refactor/descriptor-families` replace
38 lines with 72, for +34 net: the first walker occupies
`refactor/descriptor-families/v6/prolog/lower.pl:912-937`; the second occupies
`:984-1011`. PR #114's first serializer conversion produced -7. These measured
receipts are used below instead of extrapolating from visual repetition.

Counting convention: "family mass" is the source lines that the proposed
shared representation could replace, including clause heads and bodies but
excluding comments. "Overhead" includes the shared predicate or loader and
changed call sites. Net is `family mass - overhead`. A range is used only when
the exact implementation choice changes the required adapters; both endpoints
are counted from the quoted source shape.

## Decisions

1. Dispatch only slices with a counted positive net above incidental formatter
   movement.
2. Keep the parser production-table rewrite, JSON pattern table, declaration
   loop fusion, and remaining serializer-table families out of implementation.
3. Retain oracle mirrors and conformance fixtures without consolidation.
4. Treat moving clauses to data files as Prolog-line displacement. Total
   repository lines and the generated in-memory Prolog terms remain explicit.

Rejected alternatives: whole-file percentage estimates, descriptor conversion
without a per-family walker charge, manifest-row counts as behavior proof, and
combining an oracle with the compiler implementation.

## Sized dispatch table

| candidate slice | counted family | overhead and net | identity gate | verdict |
|---|---|---|---|---|
| Delete duplicate `sql_template_array/2` | 2 predicates, 8 lines: `sql_template_array_text/2` at `v6/prolog/emit_ts.pl:1127-1130` and the identical `sql_template_array/2` at `v6/prolog/emit_ts.pl:1278-1281`; representative: `maplist(js_template, Sqls, Templates)` | 0 new-family lines; rename 3 call sites at `v6/prolog/emit_ts.pl:1267-1269`; net **4 lines** | Run sweep, then require clean `git status --short v6/prolog/compile/out`; the sweep owns `compile/out` at `v6/prolog/sweep.pl:29-37` | **dispatch, luna small** |
| Share `rule_is_edge/1` and `rule_body/2` from program checks | 5 duplicate lines: analyzer at `v6/prolog/analyze.pl:62-72`, checker at `v6/prolog/0_program_check.pl:804-807`; representative: `rule_body((_ <+ Body), Body).` | export 2 predicates and extend the existing analyzer import at `v6/prolog/analyze.pl:35-36`: 3 lines; net **2 lines** | plunit plus conformance; these are behavior helpers and do not emit text. The round-trip gate invokes conformance at `v6/prolog/compile/scripts/roundtrip.sh:240-257` | **skip, near-zero** |
| Share `column_type_decls/3` | 8 lines across `v6/prolog/0_ast_expand.pl:220-223` and `v6/prolog/1_host_expand.pl:566-569`; representative: `col_type(Ref, Name, Type)` | a numbered shared module, exports/imports, and the 4-line walker cost at least 10 lines; net **at most -2** | conformance and plunit | **skip** |
| Share `build_rule/4` | 4 lines across `v6/prolog/0_ast_expand.pl:258-259` and `v6/prolog/0_coalesce_expand.pl:103-104`; representative: `build_rule(level, Head, Body, (Head <- Body)).` | shared module plus two imports is at least 8 lines; net **at most -4** | conformance and plunit | **skip** |
| Share identity `memberchk_eq/2` | 4 lines across `v6/prolog/0_coalesce_expand.pl:179-180` and `v6/prolog/0_dot_expand.pl:647-648`; representative uses `Head == Variable` | shared module plus two imports is at least 8 lines; net **at most -4** | conformance and plunit; variable identity must remain `==` | **skip** |
| Fuse catalog rel-id passes | 10 lines in `catalog_rel_id_map/4` and `catalog_rel_block_end/3` at `v6/prolog/lower.pl:1367-1383`; representative increment is `Id0 + 1 + RelArity` | one combined traversal is 7-9 lines plus 2 changed calls at `v6/prolog/lower.pl:1223-1224`; net **-1 to 1** | conformance and plunit; catalog ids and order are behavioral | **skip, near-zero** |
| Fuse primitive/list catalog row walkers | 8 walker lines at `v6/prolog/lower.pl:1312-1315` and `v6/prolog/lower.pl:1349-1356`; representative output is an accumulated `row/11` | parameterized walker requires 7-9 lines plus two adapters because primitive reverses an accumulator while list threading grows an id map; net **-3 to 1** | conformance and plunit | **skip** |
| Share snapshot entry renderer | 10 lines at `v6/prolog/emit_ts.pl:901-905` and `v6/prolog/emit_ts.pl:928-933`; representative output is `select_rows(seam, Template, rel_columns.Name!, rel_column_types.Name!)` | selector plus common formatter is 7-9 lines and 2 maplist call changes; net **-1 to 1** | sweep plus clean `compile/out` status | **skip, near-zero** |
| Share optional serializer null cases | 6 null clauses at `v6/prolog/emit_ts.pl:1211-1214`, `:1232`, `:1247`, `:1285`, `:1309`, `:1332`, `:1335`, and `:1368`; representative is `(none, null) :- !.` | higher-order dispatch needs one wrapper plus changed calls and does not remove the non-null clauses; 8-12 lines; net **-6 to -2** | sweep plus clean `compile/out` status | **skip** |
| A/B declaration column loop | 8 lines at `v6/prolog/compile/parse_dl.pl:676-686` and `:865-890`; representative is comma-tail recursion | shared loop needs a parser callback and adapters because A admits an omitted type while B records wrappers before the common type grammar; 10-14 lines; net **-6 to -2** | `roundtrip.sh`: binding, real-file parse, and conformance are enumerated at `v6/prolog/compile/scripts/roundtrip.sh:2-11` | **skip** |
| Parser lexeme wrapper | 226 source lines call `ws0/2` or `lit_dcg/3` in `v6/prolog/compile/parse_dl.pl`; representative A declaration sequence at `:622-643` has 8 such lines | `lexeme//1` plus conversion retains state variables and consumes about 2 lines per converted call; counted convertible mass is 226 lines, replacement 226 lines plus a 2-4 line helper; net **-4 to -2** | text-door receipt plus plunit and conformance; the text receipt has a bounded runner at `v6/prolog/compile/scripts/text_door_receipt.sh:3-18` | **skip** |
| Parser production table | 184 predicate names in the parser; declaration, expression, brace, and statement productions have different result/state signatures. Representative statement dispatch is `v6/prolog/compile/parse_dl.pl:584-593`; recursive variable threading appears at `:2004-2021` | a row table can replace only word-to-constant leaves such as the 7 type clauses at `:697-731`; a generic production interpreter and typed adapters price at 30-40 lines per signature family. The 7-line leaf saves at most 7 against 30-40, net **-33 to -23** | text-door receipt, plunit, round-trip, conformance | **skip** |
| JSON pattern operation table | 6 dispatch shapes in `json_pattern_sql/8` at `v6/prolog/lower.pl:4661-4743`; representative spread recursively threads position, index, bindings, FROM parts, and WHERE parts at `:4715-4727` | only empty-object and literal guards are stateless. A descriptor walker must preserve 8 arguments and per-op recursion; 30-40 fixed lines against 10 replaceable dispatch lines, net **-30 to -20** | conformance and plunit; this changes emitted SQL, so also sweep plus clean `compile/out` status | **skip** |
| Lower SQL SELECT-shape helper | 12 repeated branch lines in edge projection at `v6/prolog/lower.pl:2879-2884`, fixpoint arms at `:3929-3931` and `:4014-4016`, and level selection at `:4520-4522`; representative chooses SELECT with/without FROM and WHERE | one 4-way helper is 8-10 lines plus 4 call adapters; net **-2 to 0** | sweep plus clean `compile/out` status and SQL-text plunit snapshots | **skip, near-zero** |
| Normalize ordered trigger kinds once | 3 clauses at `v6/prolog/emit_ts.pl:1756-1758`; the lowerer produces arrival kinds at `v6/prolog/lower.pl:2800-2807` | moving the normalization into the IR changes producer and every consumer; at least 4 changed lines for 3 removed; net **at most -1** | sweep plus clean `compile/out` status | **skip** |

## Never-audited files

The name-level audit found 117 predicate names in `analyze.pl` and 43 in
`0_program_check.pl`. Their intersection is exactly `rule_is_edge/1`,
`rule_head/2`, and `rule_body/2`. `rule_head/2` has four live analyzer clauses
at `v6/prolog/analyze.pl:65-72`; the checker uses its local `head_ref/2` and
pattern matches heads at `v6/prolog/0_program_check.pl:62-82`. The only literal
dedup priced into the table is therefore the 5-line edge/body slice.

The larger apparent overlap is already centralized: `analyze.pl` imports
`first_violation/3`, `relation_kind/3`, and `declared_key/3` from the checker at
`v6/prolog/analyze.pl:35-36`. The checker owns the ordered violation driver at
`v6/prolog/0_program_check.pl:34-41`, relation-kind fallback at `:46-60`, regexp
validation at `:299-321`, and relation-pattern projections at `:762-824`.
No second implementation of those families remains in `analyze.pl`.

## Remaining emitter serializer families

Each row is priced independently. PR #114 already converted the fixpoint IR
family at `v6/prolog/emit_ts.pl:1283-1421`; it is recorded as done and excluded
from dispatch.

| serializer family | counted mass | table/walker price and net | verdict |
|---|---|---|---|
| Snapshot fields/read entries | 18 serializer lines at `v6/prolog/emit_ts.pl:868-875`, `:901-905`, and `:928-933` | two output shapes and SQL-field selection require 14-18 lines; net **0-4** | skip |
| Incremental statement entries | 48 serializer lines at `v6/prolog/emit_ts.pl:1113-1191` and retention entry `:1203-1209` | three term arities, conditional intern fields, key indices, and recompute SQL require 35-45 walker/adapter lines; net **3-13** | skip; below the measured 2x overhead threshold |
| Refcount/expand/dred SQL objects | 68 lines at `v6/prolog/emit_ts.pl:1214-1281` | three distinct term shapes and ordered field names require 55-70 lines; net **-2 to 13** | skip |
| Aggregate SQL objects | 26 lines at `v6/prolog/emit_ts.pl:1426-1451` | shared fields already occupy 18 repeated lines, but `intern_sql` and `delta_maintained` differ; descriptor plus walker prices at 30-40; net **-14 to -4** | skip |
| Edge resolver block | 126 executable lines at `v6/prolog/emit_ts.pl:1482-1643` | output is generated control flow with set/log and departure/arrival branches, rather than repeated records; a descriptor would retain the branches and add 30-40 lines; net **-40 to -30** | skip |
| Ordered arm and occurrence entries | 55 serializer lines at `v6/prolog/emit_ts.pl:1756-1816` | four output shapes plus SQL construction price at 35-45 lines; net **10-20** | skip; mass does not exceed 2x fixed overhead |

## Parser/printer symmetry

The printer projects operator precedence directly from `expression/5` at
`v6/prolog/print_dl.pl:604-606`; that completed rank 3. Declaration printing is
asymmetric by construction: it mines and augments declarations before rendering
at `v6/prolog/print_dl.pl:172-197`, while the parser records column order and
surface-presence at `v6/prolog/compile/parse_dl.pl:622-643`. The common type
vocabulary is already one parser predicate at `parse_dl.pl:690-731`, while the
printer has a 7-line inverse at `print_dl.pl:310-321`. A shared table would
replace those 7 printer lines plus 7 parser leaf clauses, 14 lines total, with a
30-40 line descriptor/walker family. Net is -26 to -16, so this pair is skipped.

Modifier symmetry is 12 printer lines at `v6/prolog/print_dl.pl:338-354` against
13 parser lines at `v6/prolog/compile/parse_dl.pl:662-674` and `:807-818`.
Ordering, optionality, and exact surface-presence live in the parser loop; the
printer consumes stored declarations. A table would still need both walkers,
pricing at 30-40 lines each against 25 lines of clauses. Net is -55 to -35, so
this pair is skipped.

## Clauses that could leave Prolog

`registry.pl` contains 132 single-row data clauses: 59 `surface/5`, 17
`expression/5`, and 56 clock/bind/host/route/CLI/trace rows. Representative
surface rows are `v6/prolog/compile/registry.pl:128-138`; expression rows are
`:236-263`; bind rows are `:290-294`. A plain-data loader must decode nested
roles such as `refs_of_arg(1,pos,sampled)` and `wrapper(rel_atom,lower)`, then
reconstruct the same callable terms used by `surface_for_term/6` at
`v6/prolog/compile/registry.pl:265-268` and its surface equivalent.

Pricing one JSON data file gives 132 displaced Prolog lines, 132 data rows, and
an estimated 28-36 Prolog loader/schema/conversion lines. Prolog-line net is
96-104; repository-line net is -36 to -28 before formatting expansion. The
oracle stays homoiconic only if the loader reconstructs the exact compound
roles before any query. Verdict: **skip** because this moves syntax across a
loader seam while increasing total lines. It remains a priced displacement,
not an implementation recommendation.

Catalog contract rows in `lower.pl` are less favorable. The contract starts at
`v6/prolog/lower.pl:683`; later catalog families require live IDs, schema hashes,
and conditional rows, as shown by primitive/list generation at `:1310-1363`.
Only the primitive names are plain data, 5 entries against a loader of at least
8 lines. Net is at most -3. Verdict: **skip**.

## Verification

For the sole dispatchable slice:

1. Capture `git status --short v6/prolog/compile/out`.
2. Run the compiler sweep; it writes the directory declared at
   `v6/prolog/sweep.pl:29-37`.
3. Require `git status --short v6/prolog/compile/out` to match the pre-run
   capture byte for byte.
4. Run plunit.
5. Run `bash v6/prolog/compile/scripts/roundtrip.sh`; its G1/G2/G3 contract is
   stated at `v6/prolog/compile/scripts/roundtrip.sh:2-11` and its final
   pass/fail reduction at `:257-270`.

No manifest count substitutes for the output-tree status comparison.

## Staffing

Base SHA: `b3c2b711`.

| slice | model | worktree | suite budget |
|---|---|---|---|
| delete duplicate `sql_template_array/2` | luna small | yes | 5 minutes for sweep, plunit, round-trip, and output-tree comparison |

All skipped rows receive no lane. The implementation lane must remain one
slice so the four-line deletion has an attributable byte-identity receipt.
