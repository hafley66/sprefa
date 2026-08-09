# Refusal revival triage: all 101 named refusals

Every named refusal in `plans/2026-08-01-refusal-inventory.md` re-opened, traced
to its live throw site on `main`, and classified. Throw-site line numbers in the
inventory are stale (written 2026-08-01, `prolog_folder_flatten` and several arcs
moved them since); every line number below was re-grepped on `main` at
2026-08-08.

Identifier spelling: this doc cites the THROWN TERM (`unsupported_construct/1`
and its reason functors), which is stable. A concurrent lane on
`lane/unsupported-rename` is renaming the noun stem `refusal` -> `unsupported`
in predicate and file names in `v6/prolog` + `v6/tsv2`; `main` is untouched and
every citation in this doc is a `main` citation.

## Table of contents

- [Method and class scheme](#method-and-class-scheme)
- [Counts](#counts)
- [The 101 rows](#the-101-rows)
- [Addendum: refusals live on main that the inventory never rowed](#addendum-refusals-live-on-main-that-the-inventory-never-rowed)
- [Design sketch 1: string split / substr](#design-sketch-1-string-split--substr)
- [Design sketch 2: scan-into-json (`json_value_expression`)](#design-sketch-2-scan-into-json-json_value_expression)
- [Design sketch 3: `module_path_unresolved`](#design-sketch-3-module_path_unresolved)
- [Design sketch 4: `group_concat/1` + `json_array/1`](#design-sketch-4-group_concat1--json_array1)
- [Dispatch order](#dispatch-order)

## Method and class scheme

Sources read: every `throw(unsupported_construct(...))` site in `v6/prolog`
(248 occurrences, 111 distinct reason functors), the non-`unsupported_construct`
throw families in `1_host_expand.pl` / `conformance/engine.pl` / `use_resolve.pl`,
`v6/prolog/compile/out/manifest.json` (320 fixtures, 220 `compiled` /
100 `unsupported`, 54 distinct reason functors in the unsupported bucket),
`v6/prolog/conformance/rulings.pl` (99 `ruling/4` rows), and
`v6/prolog/0_refusal_messages.pl` (the renderer, which derives its inventory by
scanning clause bodies of 14 declared source modules rather than keeping a list).

| class | meaning |
| --- | --- |
| a | phase-order or plumbing accident. A fall-through, a missing table row, or a stage that runs before the fact it needs exists. Revivable without a design decision. |
| b | unfinished work. A lowering that can be written and has not been. Needs a feature arc, not a debate. |
| c | encodes a real impossibility or a stated invariant. The invariant is named in the row with its citation. |
| d | already revived, dead, a lab file, or not a language decision. |
| e | user-ruled. A `rulings.pl` row is Chris's word; reviving needs his say-so first. |

The four invariants that appear in class (c), with where each is written down:

| invariant | where it is stated |
| --- | --- |
| stratification / termination | `analyze.pl:1630-1634` (aggregate strictly above every body ref, citing `level_eval.pl` `rule_body_constraint/4` Gap=1); `strat.pl:98`; V-009 tabling verdict (100/100 byte-identical + one adversarial tripwire where the tabled run wrongly derives both `p` and `q`) |
| tick-plane separation | `compile/registry.pl:215-221` `clock_role/4`: a level body has only `level_read`/`level_absence` at ring `b` delay 0; `latest` = `edge_sample`, `pre` = `edge_pre` delay -1, `finalize` = `edge_departure` delay 1 |
| id-plane integrity | `lower.pl:289-300`, measured through sqlite 3.45.1: TEXT `'1'` joins INTEGER `1` in SQL and derives nothing in the oracle; `analyze.pl:800-806` `merge_type/3` on `ref(Type)` |
| tick-log purity | ruling `json_ticklog_encoding` (canonical JSON text, values never ids); `0_type_plane.pl:129-133` states the ids-in-a-list consequence directly |

Two-door agreement is a fifth reason that recurs and is NOT one of the four
named invariants. It appears in rows where the compiler refuses something the
reference engine (`conformance/engine.pl`, `level_eval.pl`, `body.pl`) also
refuses, so the refusal keeps the two doors byte-identical rather than
protecting semantics. Rows carrying it are marked in the verdict text.

## Counts

| class | rows | share |
| --- | --- | --- |
| a: plumbing accident | 14 | 13.9% |
| b: unfinished work | 30 | 29.7% |
| c: real invariant | 39 | 38.6% |
| d: dead / not a decision | 9 | 8.9% |
| e: user-ruled | 9 | 8.9% |
| **total** | **101** | |

Effort, class a + b only (44 rows): S 22, M 21, L 1.

Corrections the trace forced on the inventory:

| inventory claim | what `main` says |
| --- | --- |
| 101 rows is the refusal set | 111 distinct reason functors exist on `main`; at least 25 postdate the inventory (addendum below) |
| N-033 `edge_body_needs_negation`, N-034 `edge_body_needs_now` are live refusals | Both were REMOVED by the `edge_body_constructs` arc (`ARCH.pl:201` names all four `edge_body_needs_*` as removed). Only plunit fixtures mention them. |
| N-078/N-091/N-100 are compiler refusals | All three live in `v6/prolog/labs/`. Lab protocol says labs die on landing; these are not language decisions. |
| N-080 `param_count_mismatch` is a compiler refusal | It is a check inside `compile/test/run_sql_check.pl`, the SQL-agreement harness. |
| N-014 `at` is a refusal | It is the `at(File, Line, Reason)` location wrapper (`compile.pl:379-390`). The inventory already suspected this. |
| N-079 `oracle_refuses_live_capture_type` is its own row | It is the plunit name for what production throws as `json_capture_type_unknown` (N-052). |

## The 101 rows

| id | reason functor | throw site (main, 2026-08-08) | class | effort | seam a revival lane edits | verdict |
| --- | --- | --- | --- | --- | --- | --- |
| N-001 | `aggregate_group_not_delta_local` | `lower.pl:2830` | b | M | `lower.pl` `aggregate_scope_group_exprs/5` | Renames an `unbound_head_var` caught out of the group-expression compile. The group key is not reachable from the delta alias. Seeding the scope table from the stored rel instead of the delta widens it. |
| N-002 | `aggregate_head(json_array/json_object)` | `analyze.pl:1582` via `compile/registry.pl:167-168` | b | M | `registry.pl`, `lower.pl` `aggregate_select_expr/5` | Ruling `q9_aggregate_heads` RESERVES both names as head-position forms; `analyze.pl:1680-1688` says the oracle computes them and only the compiler refuses. Downstream of sketch 2. SKETCH 4. |
| N-003 | `aggregate_head_mixed_with_plain_clause` | `lower.pl:2409` | b | M | `lower.pl` level statement builder | An aggregate head maintains by group-scoped recompute, a plain head by delta insert; one table runs one plan today. A union of the two plans is writable. |
| N-004 | `aggregate_head_no_positive_body` | `analyze.pl:1639` | c | S | `analyze.pl` `check_aggregate_rule_shape/2` | Invariant, scope seeding: the scope table is seeded by `SELECT DISTINCT <group> FROM <delta>` (`lower.pl:2825-2830`). With no positive body ref there is no delta and no group to recompute. |
| N-005 | `aggregate_head_reads_itself` | `analyze.pl:1635` | c | L | `analyze.pl`, `conformance/level_eval.pl` | Invariant, stratification. `analyze.pl:1630-1634` cites `level_eval.pl` `rule_body_constraint/4` Gap=1 for every body ref of an aggregate head, so a self-read is `not_stratified` on the oracle too. |
| N-006 | `aggregate_head_shape` | `0_program_check.pl:641` | a | S | `compile/registry.pl` `surface/5` | Fires when an `ordered_aggregate_name` (`json_group_array`, `group_concat`) appears at an arity with no `surface/5` row. Adding the row un-refuses it. `group_concat/1` is exactly this shape. |
| N-007 | `aggregate_in_edge_head` | `0_program_check.pl:637` | c | L | `0_program_check.pl`, `lower.pl` `edge_statements_for_rule/4` | Invariant, tick plane: an edge arm writes one row per occurrence (ruling `q4_edge_propagation`) and has no group scope at write time. The rewrite is a log rel plus a level aggregate. |
| N-008 | `aggregate_kind_not_lowered` | `lower.pl:4330` | a | S | `lower.pl` `aggregate_select_expr/5` | Catch-all after the five lowered kinds. Every registry aggregate row needs one clause here; the row states nothing about the language. |
| N-009 | `aggregate_operand_not_number` | `lower.pl:4355`, `0_program_check.pl:713` | c | S | `lower.pl` `compile_aggregate_number_operand/5` | Two-door agreement, with a receipt: V-063's 4-line repro shows the oracle dying inside `lists:min_list/3` on a TEXT column. `0_program_check.pl:692-700` states that as the reason the class exists on the shared trigger. |
| N-010 | `aggregate_ordinal_not_int` | `lower.pl:4342` | b | S | `lower.pl` `compile_aggregate_ordinal_operand/4` | The `ORDER BY` operand is pinned to `int`. Any sortable column would lower; SQL orders TEXT fine. |
| N-011 | `aggregate_separator_not_constant` | `lower.pl:4348` | b | S | `lower.pl` `compile_aggregate_text_separator/2` | Separator must be a ground non-numeric atom. SQLite's `group_concat` takes an expression, so a bound column lowers with no new SQL. |
| N-012 | `arith_operand_not_int` | `lower.pl:590` | c | M | `lower.pl` `compile_int_operand/5` | Two-door agreement, measured: `lower.pl:481-487` records `engine.pl` `eval_int2/4` throwing where SQLite silently coerces (`'not_a_number' + 1` evaluates to 1). Only `mod` reaches it since the both_number widening. |
| N-013 | `arith_operand_not_number` | `lower.pl:600` | c | S | same | Same reason, the float-widened sibling. |
| N-014 | `at` | `compile.pl:379-390` | d | - | - | The `at(File, Line, Reason)` location wrapper, not a decision. |
| N-015 | `bind_mismatch` | `1_host_expand.pl:400`, `emit_ts.pl:468` | a | S | `compile/registry.pl` `bind_definition/2` | A declared bind whose column list disagrees with the registry row. Adding a `bind_definition/2` row is the whole fix; `interval` and `watch` are the only two rows today. |
| N-016 | `coalesce_in_head` | `0_coalesce_expand.pl:262` | b | M | `0_coalesce_expand.pl` | `coalesce/2` desugars into a body probe and a head occurrence has no goal slot. Hoisting to a fresh body bind is writable. |
| N-017 | `coalesce_multiple_outputs` | `0_coalesce_expand.pl:161` | b | M | same | One hole per `coalesce` today. N holes lower as N probes. |
| N-018 | `coalesce_no_output` | `0_coalesce_expand.pl:157` | c | S | same | Invariant, construct identity: `coalesce/2` IS the hole. With zero unbound variables there is nothing to default. |
| N-019 | `coalesce_not_top_level` | `0_coalesce_expand.pl:256` | b | M | same | A nested `coalesce` reaches `analyze.pl` where the registry's `refs_of_arg` role reads the source as an ordinary join and the default vanishes silently (comment at `:250-255`). Recursing the expansion is the fix. |
| N-020 | `coalesce_output_not_column` | `0_coalesce_expand.pl:166` | b | S | same | The hole must sit in a bare argument position. A hole inside a compound needs the destructure path, which is sketch 2's territory. |
| N-021 | `coalesce_source_not_rel_atom` | `0_coalesce_expand.pl:149` | b | M | same | Source restricted to a plain relation atom. A host probe or a `decode` source needs its own probe plan. |
| N-022 | `column_mismatch` | `1_host_expand.pl:215`, `:236` | c | S | `1_host_expand.pl` | Invariant, template substitution: a host column name cannot repeat and cannot be both input and output. The template has one slot per name. |
| N-023 | `column_ref_type_conflict` | `analyze.pl:806` | c | M | `analyze.pl` `merge_type/3` | Invariant, id-plane integrity. A `ref(Type)` column stores dictionary ids; merging it with any other type puts two id spaces in one column. |
| N-024 | `column_type_unknown` | `0_type_plane.pl:128`, `0_program_check.pl:342`, `lower.pl:1908` | d | S | `0_type_plane.pl` `column_storage/3` | The precedent row. The phase-order half was traced and fixed 2026-08-08. Residue is the honest "no such declared type name" case; the manifest's 2 rows are both the typo `spann`. |
| N-025 | `comparison_type_mismatch` | `lower.pl:1004` | c | M | `lower.pl` `check_comparison_types/4` | Invariant, id-plane integrity. Sibling of N-051 with the same sqlite 3.45.1 receipt at `lower.pl:289-300`. |
| N-026 | `concat_non_display_piece` | `lower.pl:641` | c | S | `lower.pl` `compile_concat_part/5` | Two-door agreement: `engine.pl` `text_piece/2` throws `non_display_in_concat` on a compound; an int piece auto-converts on both sides. |
| N-027 | `concat_not_a_list` | `lower.pl:632` | c | S | same | Shape guard on `concat/1`'s own argument. |
| N-028 | `decode_field_unknown` | `lower.pl:1936` | c | S | `lower.pl` `decode_pattern_atoms/5` | Invariant, decl agreement: a `decode/2` key must name a column of the declared struct type. Manifest carries `decode_field_unknown(span,beginning)`, a typo. |
| N-029 | `decode_pattern_not_object` | `lower.pl:1930` | b | S | same | Pattern pinned to `{}`/1. The SOURCE arm already admits `list(_)` (`lower.pl:3961`); the PATTERN arm never learned array patterns. Real gap. |
| N-030 | `decode_source_not_bound` | `lower.pl:3955` | c | S | `lower.pl` `compile_json_decodes/7` | Range restriction: the json source must be bound by an earlier goal. |
| N-031 | `decode_source_not_struct` | `lower.pl:1905`, `:3959` | b | M | same | Source must be ref-typed, `json`, or `list(T)`. The comment at `:1899-1903` names the open piece as SLOT-TERM-STRUCT (an encoding decision for untyped json), which is a decision to take, never an impossibility. |
| N-032 | `edge_body_multiple_finalize` | `analyze.pl:885` | b | M | `analyze.pl` `edge_trigger_shape/2` | One departure trigger per edge arm. Two `finalize` atoms is a join across two departure frontiers, and the per-rel `__departure_frontier_<rel>` TEMP tables already exist (`ARCH.pl:202`). |
| N-033 | `edge_body_needs_negation` | plunit only (`compile/test/plunit_tests.pl:1230`) | d | - | - | REMOVED by the `edge_body_constructs` arc (`ARCH.pl:201`). No production throw site. |
| N-034 | `edge_body_needs_now` | plunit only (`compile/test/plunit_tests.pl:1279`) | d | - | - | Same arc, same removal. |
| N-035 | `edge_body_with_latest` | `analyze.pl:964` | b | M | `analyze.pl` `edge_goal_refusal/4` | Guards `latest/1` in a shape the `sampled_conjunction` path does not cover. The edge-body latest sample itself landed (`ARCH.pl:201`, `latest_edge_sample`), so this is residue of a partly-landed arc. |
| N-036 | `edge_body_with_negation` | `analyze.pl:996` | b | M | same | `not/1` beyond one plain atom. Widening is one `NOT EXISTS` per negated conjunct. 1 manifest row. |
| N-037 | `edge_body_with_now` | `analyze.pl:988` | b | S | same | `now/1` in a shape the guard fold does not reach. The emitted tick counter already exists from the same arc. |
| N-038 | `edge_head_column_type_mismatch` | `analyze.pl:1071` | c | M | `analyze.pl` | Invariant, id-plane integrity across the arrival seam. An edge head column typed differently from the body column feeding it writes one id space into another. |
| N-039 | `edge_head_conflict_risk` | `analyze.pl:1486` | b | M | `analyze.pl` `check_no_edge_head_conflict_risk/2` | A RISK test, deliberately conservative. The comment at `:1454-1470` says the compiler has no per-occurrence validation twin for `engine.pl`'s `keyed_conflict`, so it refuses the shape instead. Building that validation is the revival. |
| N-040 | `edge_into_unkeyed_set` | `lower.pl:2177` | b | M | `lower.pl` edge write plan | A Set head with no declared key has no row identity for the write. Ruling `rel_default_policy` makes a bare rel unkeyed, so this refuses a very common shape; a full-row key is the writable answer. |
| N-041 | `edge_trigger_not_log` | `lower.pl:2089` | b | M | `lower.pl` `edge_statements_for_rule/4` | `marked_single` triggers pinned to Log. A Set rel has an arrival stream too (the frontier tables exist), so the pin is a lowering gap. |
| N-042 | `enum_variant_column_shape` | `0_enum_expand.pl:165` | c | S | `0_enum_expand.pl` `variant_column/2` | Shape guard: a variant field is `name: type` (ruling `decl_column_spelling`). |
| N-043 | `enum_variant_name_collision` | `0_enum_expand.pl:89` | c | S | `0_enum_expand.pl` `validate_enum_names/1` | Invariant, name minting: the generated variant rel name must be free. Manifest carries `enum_variant_name_collision(page)`. |
| N-044 | `enum_variant_shape` | `0_enum_expand.pl:156` | c | S | same | Shape guard on the variant term. |
| N-045 | `finalize_in_level_rule` | `0_program_check.pl:144`, named at `analyze.pl:1274` | c | M | `0_program_check.pl` | Invariant, tick plane: `clock_role/4` gives `finalize` the `edge_departure` role (ring `z`, delay 1); a level body has only ring `b` delay 0 roles. Restored after a compiler regression let it through; now an agreement test (`ARCH.pl:202`). |
| N-046 | `guard_goal_shape` | `lower.pl:939` | a | S | `lower.pl` guard-goal fold | Fall-through after bind / guard / regexp. Every new body-goal family needs one clause; it names no design. |
| N-047 | `head_arithmetic` | `analyze.pl:1420` | b | M | `analyze.pl`, `lower.pl` `compile_expr/7` | `compile_head_expr` renders every compound head argument as a json1 tagged term, so `A + B` in an EDGE head would store an expression tree. A level head lowers arithmetic today. Routing edge heads through the same arm closes it. |
| N-048 | `head_expr` | `lower.pl:526` | a | S | `lower.pl` `compile_expr/7` | The final fall-through of the ONE expression compiler. This is the name a program gets for any unregistered function call, `substr(...)` included. See sketch 1. |
| N-049 | `host_executor_mismatch` | `1_host_expand.pl:208` | a | S | `compile/registry.pl` `host_executor_contract/2` | A declared host whose input list does not match its executor contract. Adding a contract row un-refuses it; the repo-scoped pair at `registry.pl:305-322` is the worked precedent. |
| N-050 | `int_out_of_range` | `compile.pl:285` | e | S | `compile.pl` `check_world_shapes/3` | Ruling `wide_int_fate`: refuse everywhere with a TODO marking the future bigint door. User word, explicit deferral. |
| N-051 | `join_column_type_mismatch` | `lower.pl:341` | c | M | `lower.pl` `join_column_types_agree/4` | Invariant, id-plane integrity, measured through the real driver at `lower.pl:289-300`. |
| N-052 | `json_capture_type_unknown` | `lower.pl:4101`, `conformance/body.pl` | b | S | `lower.pl` `json_capture_json_type/2` + `body.pl` | Capture types are `int`/`float`/`text`, one per json1 `json_type` answer. `bool` is absent because json_flex card C4 measured top-level `true` degrading to 1; storage for a json bool is an open card, not a closed one. |
| N-053 | `json_key_contains_quote` | `lower.pl:3995` | b | S | `lower.pl` `json_path_segment/2` | A `"` in a key has no unambiguous path text under the double-quoted segment spelling. Path escaping is writable. |
| N-054 | `json_key_shape` | `lower.pl:4164` | a | S | `lower.pl` `json_member_sql/9` | Fall-through after the atom-key, `$name` hole and `**` descent arms. A new key form needs one clause. |
| N-055 | `json_pattern_shape` | `lower.pl:4085` | a | S | `lower.pl` `json_pattern_sql/8` | The same fall-through on the value side. |
| N-056 | `json_value_expression` | `lower.pl:516`, predicate at `lower.pl:582-584` | b | M | `lower.pl` `compile_expr/7` | SKETCH 2. |
| N-057 | `keep_on_non_log_rel` | `0_program_check.pl:124` | e | S | `0_program_check.pl` | Ruling `q10_retention`: `keep` is a Log-only clause. |
| N-058 | `key_position_duplicate` | `0_program_check.pl:97` | c | S | `0_program_check.pl` | Invariant, key identity: a key position list is a set; a repeat names one column twice in one PRIMARY KEY. |
| N-059 | `key_position_out_of_range` | `0_program_check.pl:89` | c | S | same | Bounds check against decl arity. |
| N-060 | `keyed_conflict` | `conformance/engine.pl:416` | c | L | `conformance/engine.pl`, `analyze.pl` | Invariant, occurrence identity (ruling `q1_occurrence_identity`): one occurrence deriving two different rows for one key. The compiler has no per-occurrence twin, which is why N-039 refuses the shape conservatively instead. |
| N-061 | `keyed_level_head` | `0_program_check.pl:106` | e | S | `0_program_check.pl` | Ruling `keyed_level_head`: user chose "Compile error" over silent inert accumulation. |
| N-062 | `keyed_log_rel` | `0_program_check.pl:113` | b | M | `0_program_check.pl` | Log rels carry no key concept today (`lower.pl:2178` comment: "log: no key concept"). Ruling `one_decl_surface` records the keyed-vs-log split as disliked and slated for revisit, so this row is already scheduled to move. |
| N-063 | `latest_in_level_rule` | `0_program_check.pl:131` | c | M | `0_program_check.pl` | Invariant, tick plane: `clock_role/4` gives `latest` the `edge_sample` role (ring `b`, state); a level body runs to fixpoint with no sample point. Ruling `latest_over_log` is the neighbouring user word on the Log case. |
| N-064 | `level_body_goal` | `analyze.pl:1585` | a | S | `analyze.pl` `body_forbidden_goal/2` | Fall-through naming an unrecognized level body goal. 4 manifest rows, 2 of them `json_each(F,G)`, a construct the level path never learned. |
| N-065 | `level_rule_no_positive_body` | `lower.pl:3848` | c | S | `lower.pl` `level_insert_sql/6` | Range restriction: a level rule with only negations derives an infinite relation. |
| N-066 | `lifecycle_arm` | `0_program_check.pl:601`, named at `analyze.pl:1442` | e | M | `compile/registry.pl` | Ruling `lifecycle_arm_vocabulary`: the six rx Observer words are reserved. Fail-first receipt in the comment at `0_program_check.pl:568-578` (all five spellings answered `rows=[]` on the oracle before the class existed). |
| N-067 | `list_element_not_scalar` | `0_type_plane.pl:121` | b | M | `0_type_plane.pl` `list_element_type/2`, `lower.pl` `column_def/3` | The comment at `0_type_plane.pl:108-117` names the prerequisite: `list(T)` collapses to `json` storage, so no element guard is emitted, and carrying `list(T)` to `column_def/3` widens every site matching on `json`. |
| N-068 | `list_of_relation_refs` | `0_type_plane.pl:120` | c | M | same | Invariant, tick-log purity: ids in a list would print in the tick log, which ruling `json_ticklog_encoding` forbids. Stated at `0_type_plane.pl:129-133`. |
| N-069 | `log_on_level_headed_rel` | `0_program_check.pl:119` | c | M | `0_program_check.pl` | Invariant, rel-kind is one word doing six jobs (ruling `q3_rel_kind_shape`): a level head recomputes a set, a Log appends occurrences. |
| N-070 | `match_arm_head_not_positive_rel` | `0_match_expand.pl:105` | c | S | `0_match_expand.pl` | Shape guard: an arm head must be a rel atom to receive the arm's rows. |
| N-071 | `match_arm_shape` | `0_match_expand.pl:100`, `:128` | c | S | same | An arm is `<-` or `<+` (ruling `match_arm_tokens`). |
| N-072 | `match_nonexhaustive` | `0_match_expand.pl:119` | b | M | same | Exhaustiveness over enum variants with no default-arm spelling anywhere in the surface. A `_` catch-all arm is the writable widening. 1 manifest row. |
| N-073 | `match_source_not_positive_rel` | `0_match_expand.pl:51` | c | S | same | Shape guard, source side. |
| N-074 | `missing_retention` | `0_program_check.pl:615` | e | S | `0_program_check.pl` | Ruling `q10_retention`: `keep` required on Log rels. |
| N-075 | `negated_guard_goal` | `analyze.pl:1599` | b | S | `analyze.pl`, `1_expansion.pl` | `not(X > 1)` refused while `X =< 1` lowers. Inverting the operator at expansion is the entire fix; `registry.pl` `expression/5` already pairs every ordered comparison with its complement. |
| N-076 | `non_finite_float_literal` | `lower.pl:258` | c | S | `lower.pl` `sql_literal/2` | Invariant, storage-plane spelling: SQLite has no Inf/NaN literal. Ruling `numeric_precision` approved float/REAL, and said nothing about the non-finite class. |
| N-077 | `now_in_level_rule` | `analyze.pl:1596` | b | M | `analyze.pl`, `lower.pl` `level_insert_sql/6` | The comment at `:1588-1595` states it outright: the ORACLE solves `now/1` in a level body (`solve/2` reads the tick out of `ctx/3`), so this is a named compiler capability gap. Lowering needs the tick inside the level DELETE/INSERT pair. |
| N-078 | `openapi_type_unknown` | `labs/openapi_codegen/emit_openapi.pl:210` | d | - | - | Lab file. Not a language refusal. |
| N-079 | `oracle_refuses_live_capture_type` | plunit only (`compile/test/plunit_tests.pl:3391`) | d | - | - | Test-side alias of N-052. |
| N-080 | `param_count_mismatch` | `compile/test/run_sql_check.pl:350` | d | - | - | A check inside the SQL-agreement harness, not a language decision. |
| N-081 | `pattern_arg` | `lower.pl:309` | a | S | `lower.pl` `compile_pattern_arg/8` | Fall-through after var / `bool_lit` / compound / atomic. Reachable only by a term-door fixture writing a shape the parser cannot produce. |
| N-082 | `pre_in_level_rule` | `0_program_check.pl:136` | c | M | `0_program_check.pl` | Invariant, tick plane: `clock_role/4` gives `pre` the `edge_pre` role (ring `b`, delay -1) and ruling `r6_pre_visibility` defines `pre` against the evolving store within a tick, a position a level fixpoint does not have. |
| N-083 | `probe_mismatch` | `1_host_expand.pl:446`, `:477`, `:483`; `print_dl.pl:446` | b | M | `1_host_expand.pl` | One host probe per rule body. Two probes in one rule is a real shape (a join across two hosts) and needs two demand-rule splits. |
| N-084 | `query_mismatch` | `1_host_expand.pl:422` | c | S | `1_host_expand.pl` | Shape guard on `query/1`'s argument; ruling `zero_query_semantics` defines the construct. |
| N-085 | `quote_in_literal` | `lower.pl:261` | a | S | `lower.pl` `sql_literal/2` | A single quote in an atom. SQL escaping is doubling the quote; the clause writes the literal unescaped and refuses instead. Four lines. |
| N-086 | `recursive_stratum` | `strat.pl:98` | c | L | `strat.pl` `topo_order_group/2` | Invariant, stratification and termination. V-009 measured 100/100 fixtures byte-identical plus an adversarial tripwire where the tabled version wrongly derives both `p` and `q`. Narrower than "no recursion": recursive-CTE support exists separately at `lower.pl:3700-3770`. |
| N-087 | `seq_cursor_name_collision` | `0_seq_expand.pl:134` | c | S | `0_seq_expand.pl` | Invariant, name minting: the generated `seq_<head>_<pos>` cursor rel name must be free. |
| N-088 | `seq_in_level_rule` | `0_seq_expand.pl:37` | e | S | `0_seq_expand.pl` | Ruling `seq_sugar`: M2 (cursor numbering) only, M1 scan and M3 stages stay unwired. User word. |
| N-089 | `seq_partition_type_unknown` | `0_seq_expand.pl:190` | a | S | `0_seq_expand.pl` `infer_partition_type/4` | The partition column's type is read off a `col_type` decl in the body. A partition bound by a host output or an expression has no decl to read, so a legal program refuses on a phase-order accident: expansion runs before the type plane. Ask the type plane instead. |
| N-090 | `removed_word(set)` | `analyze.pl:1449`, `compile/registry.pl:186` | e | S | `compile/registry.pl` | Ruling `no_policy_suffix_words`. User word. Sibling `removed_word(scan)` is ruling `files_naming`. |
| N-091 | `sql_text_mismatch` | `labs/json_syntax/2_lowering.pl:375` | d | - | - | Lab file. |
| N-092 | `surface_findings` | `compile.pl:314`, `6_profile.pl:46` | a | S | `compile/parse_dl.pl` `finding_fact/1` | An umbrella that re-throws parser findings as one term. Each finding under it is its own decision; the umbrella itself is plumbing. |
| N-093 | `tagged_brace_reserved` | `compile/parse_dl.pl:1670` | c | S | `compile/parse_dl.pl` `refuse_tagged_brace/1` | Invariant, door agreement by term shape: `_{...}` / `Tag{...}` are SWI DICT syntax, which a `{}`/1 term can never unify with, so the term door could never agree with a text door reading them as json. `registry.pl:135-139` CARD-BRACE-TAG, settled by measurement, and reserved on purpose for the directive's future `{` use. |
| N-094 | `template_mismatch` | `1_host_expand.pl:301`, `:303`, `:305` | c | S | `1_host_expand.pl` | Invariant, template substitution: every declared input appears in the template, no output is read as one, no unknown column is named. |
| N-095 | `text_operand_not_text` | `lower.pl:550` | c | S | `lower.pl` `compile_text_operand/5` | Two-door agreement on the `text_scalar` family (`norm/1` is the only member today). Widens as sketch 1 adds rows. |
| N-096 | `trigger_arg_not_var` | `lower.pl:2349` | b | M | `lower.pl` `compile_trigger_bound/5` | A literal in a trigger argument position. 4 manifest rows, plus a dedicated fixture `door_split_trigger_literal.pl`. Lowering it = the literal becomes an equality filter, exactly the treatment `compile_pattern_arg/8` gives a level atom. |
| N-097 | `type_arrival_shape_mismatch` | `compile.pl:286`, `conformance/engine.pl:591` | e | M | `compile.pl` `check_world_shapes/3` | Ruling `type_gate_widening`: the arrival gate covers all column types at all positions and coercion follows SQLite affinity. 11 manifest rows, the largest single family. |
| N-098 | `unbound_head_var` | `lower.pl:488` | c | S | `lower.pl` `compile_expr/7` | Range restriction (datalog safety): a head variable no body goal binds ranges over the universe. |
| N-099 | `unknown_comparison_operator` | `lower.pl:986` | a | S | `compile/registry.pl` `expression/5` | Fires for any operator with no `expression/5` row. The comment at `:980-985` says adding a row is what un-refuses it. |
| N-100 | `value_template_never_shipped` | `labs/json_syntax/2_lowering.pl:102` | d | - | - | Lab file. |
| N-101 | `zip` | `0_program_check.pl:601` | e | S | `compile/registry.pl` | Ruling `zip_reserved_row`: keep the refusal, name the equijoin in the message. User word. |

## Addendum: refusals live on main that the inventory never rowed

Twenty-five reason functors throw on `main` and carry no `N-` row. They were
added after 2026-08-01. Classified the same way, without full verdict prose.

| reason functor | throw site | class | note |
| --- | --- | --- | --- |
| `module_path_unresolved` | `0_dot_expand.pl:92` | b | SKETCH 3. 3 manifest rows, 3 fixtures in `conformance/fixtures/7_module_path.pl`. |
| `unresolvable_member` | `0_dot_expand.pl:219`, `:226` | c | Range restriction: a dot chain's root must be a body-bound variable. |
| `member_not_a_goal` | `0_dot_expand.pl:158` | c | A dot chain at goal position has no value slot. Text-door programs cannot reach it (parse error); term-door fixtures can. |
| `aggregate_not_implemented` | `0_program_check.pl:679` | a | The `group_concat/1` class. SKETCH 4. |
| `built_text_in_recursive_head` | `lower.pl:3768` | b | A recursive arm lives in one `WITH RECURSIVE` statement with no place for the intern write. Blocks string-building recursion, which is what ancestor-directory closure needs (sketch 1). |
| `recursive_cte_multiple_self_reads` | `lower.pl:3714` | b | One self-read per recursive arm; SQL allows exactly one recursive reference per CTE, so widening means a second CTE. |
| `comparison_operand_not_int` | `lower.pl:991` | c | The `both_int` type rule; sibling of N-012. |
| `comparison_operand_not_number` | `lower.pl:998` | c | The `both_number` type rule; sibling of N-013. |
| `mixed_text_encoding` | `lower.pl:3467` | c | Id-plane integrity. The comment says it is unreachable while `interned_column/2` is one clause and exists so a per-column waiver fires at compile time. |
| `relation_value_in_edge_rule` | `lower.pl:1629`, `0_program_check.pl:515` | b | Comment at `:1578-1586` calls it "a capability limit, and the honest shape for one is a name". Dictionary plans are level-body-only by construction. |
| `relation_pattern_not_a_relation_value` | `0_program_check.pl:365` | c | Decl agreement. 3 manifest rows. |
| `relation_value_under_negation` | `0_program_check.pl:503` | b | A relation value inside `not/1`, whose lowering is a `NOT EXISTS` with no room for the dictionary join. |
| `relation_column_type_conflict` | `0_program_check.pl:408` | c | Id-plane integrity. |
| `head_column_type_conflict` | `0_program_check.pl:457` | c | Id-plane integrity, head side. |
| `compound_pattern_on_arrival_rel` | `analyze.pl:1562` | b | Destructuring an arrival rel's argument in a LEVEL body. The comment says `trigger_arg_not_var` owns the edge position more precisely, so the two are one arc (see N-096). |
| `decl_type_conflicts_witness` | `analyze.pl:418` | c | Decl vs literal-witness disagreement, already narrowed by `numeric_affinity_pair/2` so the reference door's accepted programs are not refused. |
| `type_cycle` | `0_program_check.pl:330` | c | Termination: a struct type reaching itself has no finite storage layout. 2 manifest rows. |
| `dynamic_relation_name` | `0_program_check.pl:552` | c | `call/N` is not a relation atom; higher-order goals have no rel plane. 2 manifest rows. |
| `reserved_rel_namespace` | `compile.pl:248` | c | `__`-prefixed names belong to the emitter (`__tick`, `__rel`, `__support_next_*`, `__departure_frontier_*`). 2 manifest rows. |
| `enum_variant_rel_collision` | `0_enum_expand.pl:114`, `:117` | c | Name minting, generated-name side of N-043. |
| `coalesce_default_not_literal` | `0_coalesce_expand.pl:234` | c | A free variable in a refusal payload cannot be written in a fixture (`engine.pl` grades `throws/1` by `==/2`). Grading mechanics, stated in the comment. |
| `retention_head_conflict_risk` | `0_program_check.pl:626` | e | Ruling `bounded_log_arm_order`: user said "refuse it", measured zero tracked programs carry the shape. |
| `edge_body_needs_json_destructure` | `analyze.pl:1000` | b | The single biggest manifest family at 9 rows, all ghcacher-shaped (`decode` inside an edge body). Same root as N-047/`relation_value_in_edge_rule`: dictionary and json plans are level-only. |
| `host_column_shadows_runtime`, `duplicate_host_decl`, `bind_repo_column` | `1_host_expand.pl` | c | Host-decl name hygiene, 4 manifest rows between them. |
| `regexp_pattern_outside_subset`, `regexp_pattern_invalid` | `0_program_check.pl:157`, `:165` | c | Two-door agreement on the regexp subset both engines can run. |

`edge_body_needs_json_destructure` deserves a call-out that the inventory could
not make: at 9 of the manifest's 100 unsupported rows it is the single most
frequently hit refusal in the corpus, and every one of those 9 rows is a
ghcacher-shaped program. It shares a root cause with N-047, N-096 and
`relation_value_in_edge_rule`: the dictionary and json lowering plans are
level-body-only by construction, so every json or struct read in an edge body
refuses. That is one arc, not four.

## Design sketch 1: string split / substr

### Throw site

There is no dedicated throw site, and that is the finding. `substr`, `split`,
`replace`, `instr` and `trim` have no `expression/5` row in
`compile/registry.pl`. The whole text-scalar family is one row:

```prolog
% compile/registry.pl:251
expression(norm/1,    text_scalar,         3, ascii_alnum_lower,     text_only).
```

`lower.pl:548-551` matches the family at ARITY ONE only:

```prolog
text_scalar_expr(Expr, Function, Argument) :-
    compound(Expr), Expr =.. [Function, Argument],
    expression(Function/1, text_scalar, _, _, _).
```

So a program writing `Dir := substr(Path, 1, 3)` falls through every arm of
`compile_expr/7` and lands on the generic compound arm at `lower.pl:518-525`,
which encodes the CALL as a json1 tagged term
(`json_object('fn','substr','args',json_array(...))`) and stores that text.
A program writing it at goal position lands on `guard_goal_shape` at
`lower.pl:939`. Neither names strings.

The standing decision to re-open is V-028 (`plans/2026-07-29-comment-node-verdict.md`):
text-manipulation operators do not return as language constructs, all move into
host templates, evidence = 745/745 byte parity on `comment_node`. That
measurement is real and it measured ONE program. It did not measure path-prefix
derivation, which is where the gap bites: `conformance/fixtures/9_pr_size.pl`
declares `in_dir/2` at lines 7-8 and SUPPLIES it as two ground facts at line 29,
because the program cannot derive a directory from a path.

Class: (b), unfinished work. Effort M for the scalar half, L with the closure half.

### Seam

| file | edit |
| --- | --- |
| `compile/registry.pl` | new `expression/5` rows; a `text_scalar` family that admits arity 2 and 3 |
| `lower.pl:548-551` | `text_scalar_expr/3` generalized to any arity, argument list rather than one argument |
| `lower.pl:553-565` | `compile_text_operand/5` per-argument, `text_scalar_sql/3` takes a list |
| `conformance/body.pl` | oracle `eval_expr/2` clauses, one per new function, clause-for-clause |
| `print_dl.pl` | precedence rows so the round-trip check holds |
| `conformance/fixtures/9_pr_size.pl` | `in_dir/2` becomes derived; the removed fact lines are the receipt |

### Proposed surface

SQLite spelling first (ruling `vocabulary_tiebreak`), so every row in the table
is a function SQLite already ships and needs zero UDF (ruling `udf_residency` keeps
`@libsql`, which has no UDF registration API).

| spelling | SQL rendering | type rule |
| --- | --- | --- |
| `substr(Text, Start)` | `substr(?, ?)` | text, int -> text |
| `substr(Text, Start, Length)` | `substr(?, ?, ?)` | text, int, int -> text |
| `instr(Haystack, Needle)` | `instr(?, ?)` | text, text -> int |
| `replace(Text, From, To)` | `replace(?, ?, ?)` | text, text, text -> text |
| `ltrim(Text, Chars)` / `rtrim(Text, Chars)` / `trim(Text, Chars)` | same names | text, text -> text |
| `char_length(Text)` | `length(?)` | text -> int |

The one function with no SQLite scalar is split. Two shapes, and they answer
different questions:

**(A) scalar `split_part(Text, Separator, Index)`.** One piece, one row. Lowers
to a `substr`/`instr` composition or to the same recursive-CTE technique
`norm/1` already emits at `lower.pl:561`. Costs no new construct class.

**(B) table-valued `split(Text, Separator, Ordinal, Piece)` as a body goal.**
N pieces, N rows. This is the shape path work wants, and it lowers exactly the
way `decode` over `json_each` already lowers: a table function in the FROM list.
SQLite ships no `split` table function, so the FROM entry is a recursive CTE
generated per call site.

Recommendation as data, no verdict: (A) unblocks `9_pr_size.pl` today and needs
no new construct class; (B) is the only one that expresses ancestor-directory
closure, and it collides with `built_text_in_recursive_head` (`lower.pl:3768`),
so the closure case is a second arc regardless.

**The cheap finding.** `in_dir/2` needs no split at all. SQLite's own dirname
idiom is `rtrim(path, replace(path, '/', ''))`, two functions, both native, both
blocked purely because `text_scalar_expr/3` matches arity 1. Two `expression/5`
rows and a three-line arity generalization turn `9_pr_size.pl`'s supplied facts
into a derived rel.

### Fixture plan

| fixture | asserts |
| --- | --- |
| `conformance/fixtures/10_text_scalars.pl` new, one per function | both doors agree byte-for-byte on the tick log |
| same file, `substr_negative_start_counts_from_the_end` | SQLite's negative-Start semantics, pinned because the oracle must copy it exactly |
| same file, `text_scalar_on_int_column_refuses` | `text_operand_not_text` (N-095) still fires with a wider family |
| `9_pr_size.pl` edited: drop the 2 `in_dir` facts, add the derivation rule | the fixture's own final state is unchanged, which is the whole receipt |
| `10_text_scalars.pl`, `split_in_recursive_head_refuses` | keeps `built_text_in_recursive_head` red and named |

### rx lowering

Every scalar row is a pure per-row map, so it fuses into the existing per-rel pipe:

```ts
// Dir := rtrim(Path, replace(Path, '/', ''))
pr_file$.pipe(
  map(prFileRow => ({
    path: prFileRow.path,
    dir: rtrim(prFileRow.path, replace(prFileRow.path, '/', '')),
  })),
)
```

The table-valued split is the shape `decode` already has:

```ts
// split(Path, '/', Ordinal, Piece)
pr_file$.pipe(
  mergeMap(prFileRow =>
    from(pathPieces(prFileRow.path, '/')).pipe(
      map((piece, ordinal) => ({ path: prFileRow.path, ordinal, piece })),
    ),
  ),
)
```

Both write, so neither is a design defect under the every-construct-has-an-rx-lowering law.

## Design sketch 2: scan-into-json (`json_value_expression`)

### Throw site

```prolog
% lower.pl:515-516, inside compile_expr/7
    ; json_value_expr(Expr)
    -> throw(unsupported_construct(json_value_expression(Expr)))

% lower.pl:582-584
json_value_expr(Expr) :- compound(Expr), Expr = {}(_), !.
json_value_expr(Expr) :- is_list(Expr), Expr \== [], !.
json_value_expr(Expr) :- compound(Expr), Expr = [_ | _].
```

The reason is recorded at `lower.pl:576-585`. Without the refusal, the generic
compound arm at `lower.pl:518-525` encodes `{name: N, stars: S}` as
`json_object('fn',':','args',json_array('name','cli'))`, a tagged encoding of the
TERM, while the oracle stores the JSON object. The two doors disagree byte for
byte. The comment names it as the SAME cons-text encoding gap `registry.pl`
records against `json_array`/`json_object` (sketch 4).

Manifest: 2 rows in the `unsupported` bucket carry `json_value_expression`.

Class: (b), unfinished work. Effort M.

### Lift or keep write-only: the record already answers it

Ruling `json_arm` (R-014, `rulings.pl:79`) states: "json values are ordinary
terms in the one value world; `json_array`/`json_object` build them". A program
that cannot write `Doc := {name: Name, stars: Stars}` contradicts that ruling's
own sentence. The ruling side is decided; the lowering is what is missing. Lift.

### Seam

| file | edit |
| --- | --- |
| `lower.pl:515-516` | replace the throw with a json-construction dispatch |
| `lower.pl:582-584` | delete `json_value_expr/1`, or narrow it to the one case that stays refused |
| `conformance/body.pl` | confirm `eval_expr/2` already builds the term (it does; ruling `json_ticklog_encoding` gives the encoding boundary) |
| `conformance/fixtures/8_json_flex.pl` | the 2 currently-unsupported manifest rows flip to compiled |

No `registry.pl` edit: `'{}'/1` is already `value(json_object_shape)`, status
`live` (`registry.pl:128`). The surface was never the blocker.

### Proposed surface

Unchanged. `{}`/1 and list literals in any value position, which is what the
parser already produces.

### SQL lowering sketch

```
{k1: E1, k2: E2}   ->  json_object('k1', <E1>, 'k2', <E2>)
[E1, E2]           ->  json_array(<E1>, <E2>)
nested             ->  recurse; a sub-expression whose Type is json or list(_)
                       wraps as json(<sub>) so the text is not double-quoted
```

The `json(...)` wrap is not a new idea: `json_group_array_value_sql/3`
(`lower.pl:4332-4336`) already does exactly this for the aggregate case, and
that is the clause the new arm should reuse.

One case stays refused and gets its own name rather than the umbrella:
`spread(L)` inside a value literal. json1 has no positional splice, so
`{a: 1, ...Rest}` has no lowering; refuse it as `json_spread_in_value(Expr)`.

Type: the result Type is `json`. The target column must be declared `json` or
`list(T)` or the existing `type_arrival_shape_mismatch` / `column_type_unknown`
gate speaks, which is the correct division of labour.

### Fixture plan

| fixture | asserts |
| --- | --- |
| `8_json_flex.pl:json_document_built_in_value_position` | `Doc := {name: Name, stars: Stars}` compiles; tick log byte-identical across doors |
| `8_json_flex.pl:json_document_in_head_argument` | the head-position twin (`braces_in_head_position` exists and has been IDENTICAL-but-vacuous since phase C per `lower.pl:578-582`; give it a non-empty Schedule) |
| `8_json_flex.pl:json_array_literal_in_value_position` | the list arm |
| `8_json_flex.pl:nested_json_document_does_not_double_quote` | the `json(...)` wrap, the exact bug the tagged encoding caused |
| `8_json_flex.pl:json_spread_in_value_refuses` | the one case that stays red, under its own name |

### rx lowering

```ts
// Doc := {name: Name, stars: Stars}
repo$.pipe(
  map(repoRow => ({
    repoId: repoRow.repoId,
    doc: { name: repoRow.name, stars: repoRow.stars },
  })),
)
```

A pure per-row map. No new operator.

## Design sketch 3: `module_path_unresolved`

### Throw site

```prolog
% 0_dot_expand.pl:86-94
% Either door: the text door parses rel_path/2, and SWI reads `a.b(X)` as
% '.'(a, b(X)), which would otherwise become a rel literally named '.'.
refuse_rel_path_rule(Rule) :-
    (   sub_term(Sub, Rule),
        nonvar(Sub),
        rel_path_segments(Sub, Segments)
    ->  throw(unsupported_construct(module_path_unresolved(Segments)))
    ;   true
    ).
```

Called from `expand_dot_in_context/3` at `0_dot_expand.pl:63`, AFTER
`resolve_enum_arm_term/3` at `:62`. That ordering is already right:
`Enum.variant(...)` resolves first and never reaches the throw. Only unresolved
module paths get here.

Header comment `0_dot_expand.pl:28-31`: "There is no module half in scope, so a
chain whose root is not a bound body variable is never silently repairable."

Fixtures: `conformance/fixtures/7_module_path.pl`, 3 rows, all `throws(...)`.
Manifest: 3 `module_path_unresolved` rows.

Class: (b), unfinished work with a named and PARTLY-BUILT prerequisite.

### The prerequisite is further along than the comment says

Three facts the comment predates:

1. **The module id plane exists.** `lower.pl:742` emits the `__rel` catalog with
   columns `(rel_id, parent_id, ordinal, local_name, kind, type_id, arity,
   module_id, h_id, h_schema, h_rule)`. `lower.pl:748-760` mints a `module`-kind
   row per program and hangs every rel off it as a child with `parent_id`.
   `kind` ranges over `{module, rel, column, primitive}`. Dotted resolution is a
   parent/child lookup in that table.
2. **Ruling `catalog_universe` (`rulings.pl:613`) already says so**, in the
   user's own words: "i want to be able to read the rels as values/types/mods
   whatever as their own types with dots... Dot access over rels resolves
   against these rows."
3. **Ruling `block_lowering_first` (`rulings.pl:608`) already specifies the
   lowering**: children land as flat rels with long mangled names plus catalog
   rows relating them, and a captured outer arg is implicitly distributed into
   every child as a leading demand-key column.

So the design is user-ruled twice over. What is genuinely missing is A5, and A5
is smaller than "wire the use door":

**`use_resolve.pl:expand_uses/6` has ZERO production callers.** It mints
`module(EntryAbs, EntryName, EntryHash)` rows at `use_resolve.pl:80-82` and
returns them as `ModuleTable`. Every caller on `main` is in
`compile/test/plunit_tests.pl` (lines 5732, 5757, 5766, 5779, 5793, 5804, 5847,
5871). The production text door is `compile.pl:310`, which calls
`parse_dl_file/4` directly and never sees a `use` line or a module table.

That is the same shape N-024 turned out to be: a phase-order accident. The
module half is built, tested, and unreachable from the compile path.

### Seam

| file | edit | order |
| --- | --- | --- |
| `compile.pl:308-315` | route the parse phase through `use_resolve:expand_uses/6` instead of `parse_dl_file/4`; thread `ModuleTable` into the program record | 1 |
| `6_profile.pl:42` | the same swap, so the profiled path does not fork | 1 |
| `0_dot_expand.pl:88-94` | resolve `rel_path(Segments, Args)` against `ModuleTable` before refusing; keep the refusal for a path no module answers | 2 |
| `lower.pl:748-760` | `module_id` comes from the module table's hash rather than being minted per compile | 3 |
| `conformance/fixtures/7_module_path.pl` | 3 `throws` rows become 3 resolving rows plus 1 new `throws` for a genuinely unknown module | 3 |

### Proposed surface

Unchanged. `module_name.rel_name(Args)` in head or body position, already
parsed as `rel_path([module_name, rel_name], Args)` by the text door, already
term-door-reachable. Deeper paths keep every segment (the third fixture in
`7_module_path.pl` exists to pin exactly that, and its comment says the payload
"is what h4's mangler will consume").

Mangling per ruling `block_lowering_first`: `[orchard, tree]` with args
`[TreeId, Picked]` becomes the flat rel `orchard__tree/2` plus a `__rel` row
with `parent_id` pointing at `orchard`'s module row.

### Fixture plan

| fixture | asserts |
| --- | --- |
| `7_module_path.pl:dotted_head_resolves_through_the_use_door` | `use "orchard.dl6".` then `orchard.tree(X, Y) <- harvest(X, Y)` compiles and the tick log names the mangled rel |
| `7_module_path.pl:dotted_body_resolves` | body position twin |
| `7_module_path.pl:three_segments_resolve_to_a_nested_parent` | the existing three-segment row flips from `throws` to a resolution, keeping every segment |
| `7_module_path.pl:unknown_module_still_refuses_by_name` | `module_path_unresolved([nosuch, tree])` stays red |
| `plunit`: `expand_uses_is_on_the_production_compile_path` | a counter assertion so the wiring cannot silently un-wire |

### rx lowering

Module resolution happens entirely at compile time; the emitted module names one
flat rel per dotted path, so the rx side is the ordinary per-rel pipe with a
longer name. No operator, no runtime lookup:

```ts
// orchard.tree(TreeId, Picked) <- harvest(TreeId, Picked)
const orchard__tree$ = harvest$.pipe(
  map(harvestRow => ({ treeId: harvestRow.treeId, picked: harvestRow.picked })),
)
```

## Design sketch 4: `group_concat/1` + `json_array/1`

Two refusals in the same registry neighbourhood with two different causes. They
are separate arcs and the doc that treats them as one will build the wrong thing.

### Throw sites

```prolog
% compile/registry.pl:167-172
surface(json_array/1,       aggregate, no_refs, head(refuse(aggregate)),        refused).
surface(json_object/2,      aggregate, no_refs, head(refuse(aggregate)),        refused).
surface(json_group_array/1, aggregate, no_refs, head(lower),                    live).
surface(json_group_array/2, aggregate, no_refs, head(lower),                    live).
surface(group_concat/2,     aggregate, no_refs, head(lower),                    live).
surface(group_concat/3,     aggregate, no_refs, head(lower),                    live).
surface(group_concat/1,     aggregate, no_refs, head(refuse(not_implemented)),  refused).
```

- `group_concat/1` -> `0_program_check.pl:679` `program_violation(aggregate_not_implemented, ...)`.
- `json_array/1`, `json_object/2` -> `analyze.pl:1680-1688` `refused_aggregate_head_shape/2` -> throw at `analyze.pl:1582`.

`0_program_check.pl:652-658` states the difference precisely, and it is the
difference that matters: `refuse(aggregate)` means the reference engine COMPUTES
the form and only the compiler refuses, so the oracle is the wider language.
`refuse(not_implemented)` means neither door can evaluate it.

### `group_concat/1`: class (a), effort S

The fail-first receipt is in the comment at `0_program_check.pl:659-670`. Before
the row existed, `roster(group_concat(Name)) <- member(Name)` with two members
gave:

```
oracle    rows=[out(group_concat(1)), out(group_concat(2))]
compiler  COMPILED CLEAN, emitting
          json_object('fn','group_concat','args',json_array(b0."col1"))
```

One row per input holding the TEXT of the call, where the author asked for one
grouped row. The refusal replaced a plausible-looking wrong answer, which is the
right call for the state it was in.

The missing thing is a separator. `group_concat/2` and `/3` are live and lower at
`lower.pl:4317-4329`. SQLite's own one-argument `group_concat` defaults its
separator to `,`. Verdict V-052 gave `group_concat` its own aggregate spelling
and it got arities 2 and 3; arity 1 is the arity nobody wired.

Revival, in full:

| step | edit |
| --- | --- |
| 1 | `registry.pl:172`: `surface(group_concat/1, aggregate, no_refs, head(lower), live).` |
| 2 | `analyze.pl` `classify_head_arg/2` (near `:1666`): an arity-1 `group_concat` classifies as `agg(group_concat(','), Expr)` |
| 3 | `conformance/body.pl` + `level_eval.pl`: oracle computes the same default |
| 4 | nothing in `lower.pl`: `aggregate_select_expr(_, agg(group_concat(Sep), Expr), ...)` at `:4317` already handles it |
| 5 | `0_program_check.pl:679` `aggregate_not_implemented` becomes an empty class; keep it (a future refused arity lands there) |

Fixture: `conformance/fixtures/` ordered-aggregate file gains
`group_concat_one_argument_defaults_to_comma`, and a second row proving
`group_concat(X)` and `group_concat(X, ',')` produce byte-identical tick logs.

SQL lowering: `group_concat(<value>, ',' ORDER BY <value>)`, byte-identical to
what arity 2 emits today.

rx lowering:

```ts
// roster(group_concat(Name)) <- member(Name)
member$.pipe(
  scan((names, memberRow) => [...names, memberRow.name], [] as string[]),
  map(names => ({ roster: names.join(',') })),
)
```

### `json_array/1` + `json_object/2`: class (b), effort M, downstream of sketch 2

Ruling `q9_aggregate_heads` (R-009) reserves both names as head-position
aggregate forms, so the SPELLING is user-ruled and the refusal is the compiler's
own. `registry.pl:164-166` states the cause: "the compiler and oracle share the
aggregate names below. JSON values use the canonical JSON text boundary" and the
refusal is the cons-text encoding crack. `lower.pl:583-585` says the same crack
is what `json_value_expression` guards.

So the dependency is one-way and explicit: fix sketch 2 (json values lower to
real json1 constructors) and this pair's blocker is gone. The residual work is
one clause each in `aggregate_select_expr/5`:

```
agg(json_array, Expr)       ->  json_group_array(<Expr> ORDER BY <Expr>)
agg(json_object(K, V), _)   ->  json_group_object(<K>, <V>)
```

`json_group_array` already lowers at `lower.pl:4305-4309` with the
`json_group_array_value_sql/3` wrap, so the array arm is close to a rename.
`json_group_object` is a SQLite aggregate that has no live surface row at all
today, which makes the object arm the only genuinely new SQL.

The honest question this arc must answer, and it is a real one: `json_array/1`
and `json_group_array/1` would then be two spellings of one lowering. Either
`json_array/1` becomes an alias (and the registry says so in one row), or the
ruling's reserved name is retired in favour of the SQLite spelling per ruling
`vocabulary_tiebreak`. That is a user question, not an agent one.

Fixture plan: the 4 manifest rows currently carrying `aggregate_head(json_array(A))`
and `aggregate_head(json_object(A,B))` flip to compiled, each with a tick-log
byte-diff against the oracle.

rx lowering:

```ts
// tags(json_array(Tag)) <- tagged(Repo, Tag)
tagged$.pipe(
  groupBy(taggedRow => taggedRow.repo),
  mergeMap(group => group.pipe(
    scan((tags, taggedRow) => [...tags, taggedRow.tag], [] as string[]),
    map(tags => ({ repo: group.key, tags })),
  )),
)
```

## Dispatch order

Batching constraint: `lower.pl` is one 5,021-line file and `analyze.pl` is one
1,742-line file, so lanes touching either must serialize. Everything else runs
concurrently.

```
wave 1 (parallel, disjoint files)
  ├── lane REGISTRY   compile/registry.pl, 0_program_check.pl
  ├── lane COALESCE   0_coalesce_expand.pl
  ├── lane MODULE     0_dot_expand.pl, use_resolve.pl, compile.pl, 6_profile.pl
  └── lane SEQHOST    0_seq_expand.pl, 1_host_expand.pl

wave 2 (serial on lower.pl)          wave 2' (serial on analyze.pl, parallel to wave 2)
  ├── lane EXPR    lower.pl 250-660    ├── lane EDGE   analyze.pl 850-1100, 1400-1500
  ├── lane JSON    lower.pl 3950-4200  └── lane LEVEL  analyze.pl 1560-1610
  └── lane AGG     lower.pl 4300-4360

wave 3 (needs wave 2 landed)
  └── lane JSONAGG  registry.pl + analyze.pl + lower.pl aggregate arms
```

| lane | owns | rows | why these together |
| --- | --- | --- | --- |
| REGISTRY | `compile/registry.pl`, `0_program_check.pl` | N-006, N-015, N-049, N-099, `group_concat/1` half of sketch 4 | Every row is "add a table row". No lowering code changes. Cheapest wave-1 lane and the one that proves the class-a thesis. |
| COALESCE | `0_coalesce_expand.pl` | N-016, N-017, N-019, N-020, N-021 | One 274-line file, five refusals, one expansion pass. Nothing else touches it. |
| MODULE | `0_dot_expand.pl`, `use_resolve.pl`, `compile.pl`, `6_profile.pl` | sketch 3 | The compile-entry swap is the risky edit; isolate it. |
| SEQHOST | `0_seq_expand.pl`, `1_host_expand.pl` | N-089, N-083 | Two unrelated small files with no shared callers. |
| EXPR | `lower.pl` 250-660 | N-048 (sketch 1 scalars), N-085, N-095 widening | The one expression compiler. Must land before JSON, which extends the same predicate. |
| JSON | `lower.pl` 3950-4200 + 516 | N-056 (sketch 2), N-053, N-054, N-055 | The json lowering block, plus the `compile_expr` arm EXPR just widened. |
| AGG | `lower.pl` 4300-4360 | N-008, N-010, N-011 | Aggregate select expressions. Independent of EXPR/JSON in content, serialized only by the file. |
| EDGE | `analyze.pl` 850-1100, 1400-1500 | N-032, N-035, N-036, N-037, N-039 | The edge shape checker. One arc's residue (`edge_body_constructs`). |
| LEVEL | `analyze.pl` 1560-1610 | N-064, N-075, N-077 | The level shape checker. Serialized after EDGE by the file, not by content. |
| JSONAGG | `registry.pl`, `analyze.pl`, `lower.pl` | N-002 (json half of sketch 4) | Blocked on JSON landing. Carries the one user question in the batch. |

Rows deliberately not batched:

| rows | why |
| --- | --- |
| 9 class-e rows (N-050, 057, 061, 066, 074, 088, 090, 097, 101) | A `rulings.pl` row is Chris's word. Any of them can be re-opened; none should be dispatched without his say-so first. |
| 9 class-d rows | Nothing to revive. Recommend deleting N-033/N-034's plunit fixtures or relabelling them as history, and moving N-078/N-091/N-100 out of the refusal count entirely. |
| N-005, N-007, N-060, N-086 | Class c at effort L. Each needs a measured lab before an implementation lane, not a flash lane. |
| `edge_body_needs_json_destructure` + N-047 + N-096 + `relation_value_in_edge_rule` | The 9-row manifest family. One arc, "dictionary and json plans reach edge bodies", and it is the largest single win in the corpus. Wants its own opus-planned arc after wave 2. |

### Top 5 cheapest revivals

| rank | row | one-line justification |
| --- | --- | --- |
| 1 | `group_concat/1` (N-006 + `aggregate_not_implemented`) | One registry row plus one `classify_head_arg/2` clause defaulting the separator to `,`; `lower.pl:4317` already lowers the result and SQLite's own one-argument form uses the same default. |
| 2 | N-099 `unknown_comparison_operator` | The refusal's own comment says an operator with no `expression/5` row refuses by name; adding the row is the fix, with zero lowering code. |
| 3 | N-075 `negated_guard_goal` | `not(X > 1)` refuses while `X =< 1` lowers, and `registry.pl` `expression/5` already pairs every ordered comparison with its complement, so the fix is an operator flip at expansion. |
| 4 | N-085 `quote_in_literal` | SQL escaping for a single quote is doubling it; `sql_literal/2` writes the literal unescaped and refuses instead. |
| 5 | sketch 1's minimum (`rtrim/2` + `replace/3`) | Two `expression/5` rows and a three-line arity generalization of `text_scalar_expr/3` turn `9_pr_size.pl`'s two supplied `in_dir` facts into a derived rel, which is the receipt the whole string arc has been waiting on. |
