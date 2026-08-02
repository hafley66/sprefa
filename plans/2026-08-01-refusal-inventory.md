# Refusal / ruling inventory (v6 dl6 language design)

Read-only worklist. One row per design decision (ruling, named compiler
refusal, library/approach rejection) so each can be re-litigated
individually. No row here has been re-evaluated; this document only
locates and classifies.

Sources:
- `v6/prolog/conformance/rulings.pl` (81 `ruling/4` facts, rows R-001..R-081)
- Named refusals grepped from `v6/prolog/**/*.pl`: `unsupported_construct(...)`,
  `removed_word(...)`, `throw(..._refus|_conflict|_collision|_mismatch...)`,
  cross-checked against `v6/prolog/0_refusal_messages.pl` (rows N-001..N-101).
  101 unique names (`_` regex-capture artifact and the `at/3` location
  wrapper excluded as non-decisions).
- `plans/*verdict*.md`, all 23 files (rows V-001..V-063). Not every line of
  every doc is a decision; long labs with `OPEN`/`PROPOSED` slots are
  represented only where a decision, rejection, or acceptance was actually
  stated, not for every open question.

`decided-by` discipline: `rulings.pl` rows are marked `user-ruled` because the
file's own header states `RULED BY THE USER`. Verdict-doc and named-refusal
rows default to `agent-verdict` unless the cited text names the user
explicitly; `unclear` is used only where the record itself does not resolve
who accepted the finding (2 rows, both proposals marked `PROPOSED`/`OPEN` in
their own lab doc, not yet promoted to a `rulings.pl` row).

`evidence` discipline: `measurement` = a number was actually run/counted at
decision time (byte-diff, timing, corpus count). `fixture` = a named
conformance/plunit fixture asserts the behavior. `none` = the row cites only a
doc pointer, a user quote, or reasoning with no run number attached at the
time of decision. A parenthetical after the evidence class is the citation
text itself, not additional evidence.

---

| id | decision | kind | decided-by | evidence | source | re-open |
|---|---|---|---|---|---|---|
| R-001 | `q1_occurrence_identity`: hybrid_stamps_plus_support_count: (tick,seq) stamps + engine-kept refCount as the one occurrence-identity semantics | ruling | user-ruled | none (review_occurrence_identity.md:117-135, doc pointer only) | v6/prolog/conformance/rulings.pl | L |
| R-002 | `q2_scoping`: occurrence scoping (Set vs Log) is an explicit rel-kind word on the decl | ruling | user-ruled | none (review_occurrence_identity.md:35-42) | v6/prolog/conformance/rulings.pl | L |
| R-003 | `q3_rel_kind_shape`: rel-kind is one word on the decl doing six jobs | ruling | user-ruled | none (AGGREGATE.md 1b) | v6/prolog/conformance/rulings.pl | L |
| R-004 | `q4_edge_propagation`: edge-written rows are arrivals for T+1, never same-tick, never dropped | ruling | user-ruled | none (review_temporal_pipe.md:120-124) | v6/prolog/conformance/rulings.pl | L |
| R-005 | `q5_drain_scheduler`: engine self-schedules drain ticks while the carry set is nonempty | ruling | user-ruled | none (code pointer only, temporal_pipe.pl:485-486) | v6/prolog/conformance/rulings.pl | L |
| R-006 | `q6_trigger_marker`: trigger marker = explicit per-atom marker (only/1); unmarked body = any-atom | ruling | user-ruled | none (review_temporal_pipe.md:15-23; later superseded for edge bodies by the C2 unmarked-trigger ruling) | v6/prolog/conformance/rulings.pl | L |
| R-007 | `q7_aggregate_multiplicity`: aggregate multiplicity = BAG of derivations (v5-SQL-compatible) | ruling | user-ruled | none (AGGREGATE.md Q7, reasoning only) | v6/prolog/conformance/rulings.pl | M |
| R-008 | `q8_key_vs_arrow`: Key() and `->` both live: Key = undirected uniqueness on state rels, `->` = program/world split on effect rels | ruling | user-ruled | none (AGGREGATE.md Q8 option (b); later superseded in spirit by decl_column_spelling killing Key() wrappers) | v6/prolog/conformance/rulings.pl | L |
| R-009 | `q9_aggregate_heads`: count/sum/min/max/json_array/json_object reserved as head-position aggregate forms | ruling | user-ruled | none (review_expressions.md:142-151) | v6/prolog/conformance/rulings.pl | M |
| R-010 | `q10_retention`: retention = required `keep <duration\|count>` clause on Log rels only | ruling | user-ruled | none (AGGREGATE.md Q10 option (a)) | v6/prolog/conformance/rulings.pl | M |
| R-011 | `r7_boundary_diff`: tick-boundary delta = multiset diff on Log rels, set diff on Set/level rels | ruling | user-ruled | none (code/test pointer only) | v6/prolog/conformance/rulings.pl | L |
| R-012 | `r_equal_row_write`: an equal-row keyed write is a no-op | ruling | user-ruled | none ('merge ambiguity 1', no data shown) | v6/prolog/conformance/rulings.pl | S |
| R-013 | `r1_rider_pre_chains`: `pre` chains across occurrences within a tick on fold rules | ruling | user-ruled | none (code pointer, occurrence_identity.pl) | v6/prolog/conformance/rulings.pl | M |
| R-014 | `json_arm`: json values are ordinary terms in the one value world; json_array/json_object build them | ruling | user-ruled | none (plans/2026-07-27-json-arm.md, directive only) | v6/prolog/conformance/rulings.pl | L |
| R-015 | `r4_departure`: departure is bindable via a `departed/1` body form, next tick, via carry | ruling | user-ruled | none (user question quoted, no measurement) | v6/prolog/conformance/rulings.pl | M |
| R-016 | `r6_pre_visibility`: `pre` reads the evolving store (T-1 when nothing wrote yet, chains after) | ruling | user-ruled | none ('the Q1 fold correctness depends on it', reasoning only) | v6/prolog/conformance/rulings.pl | M |
| R-017 | `a6_diag`: diag is an ordinary rel declared by std/diag, never a magic rel | ruling | user-ruled | fixture ('timeless_rail fixtures model it so') | v6/prolog/conformance/rulings.pl | S |
| R-018 | `cut_pipe`: `\|>` deferred from the construct budget (zero corpus chains at the time) | ruling | user-ruled | none (AGGREGATE 1d cut order row 1) | v6/prolog/conformance/rulings.pl | S |
| R-019 | `cut_quote`: `quote()` cut; evaluation-default stays a spec sentence | ruling | user-ruled | none (AGGREGATE 1d row 2) | v6/prolog/conformance/rulings.pl | S |
| R-020 | `s2_file_rels`: file rels split (mutable worktree vs immutable tree_file) unified by the File type | ruling | user-ruled | none (fs-rev-spine S2, doc pointer) | v6/prolog/conformance/rulings.pl | M |
| R-021 | `s3_dirtiness`: dirtiness is a derived rel, no Dirty(Oid) identity | ruling | user-ruled | none (fs-rev-spine S3, doc pointer) | v6/prolog/conformance/rulings.pl | M |
| R-022 | `storage_integer_keys`: integer surrogate keys everywhere in big graph storage; strings interned once | ruling | user-ruled | none (user quote, no measurement in this ruling (see strings-n1-verdict.md for the v5-side receipts)) | v6/prolog/conformance/rulings.pl | L |
| R-023 | `n1_statement_budget`: statements/tick = f(rules,strata), never f(rows), promoted into a graded conformance rail | ruling | user-ruled | fixture ('graded by the statement-budget rail, fixture at 1x vs 100x data, identical counts') | v6/prolog/conformance/rulings.pl | L |
| R-024 | `stale_fill_policy`: stale-fill has no policy; content-addressed salts make every fill a cache update, not_applicable | ruling | user-ruled | measurement (redteam-stale-fill lab B4: three fill readings converge under content salts) | v6/prolog/conformance/rulings.pl | M |
| R-025 | `salt_minting`: salt minting is content-addressed, never a subscription id | ruling | user-ruled | measurement ('per-instance salts measured 12x world calls on the gh-cache retick probe') | v6/prolog/conformance/rulings.pl | L |
| R-026 | `effect_abort`: effect abort = best-effort cancel on demand-support-zero, never a semantic guarantee | ruling | user-ruled | none (user invariant quote, no measurement of the lowering (owed, not yet built per the ruling text)) | v6/prolog/conformance/rulings.pl | M |
| R-027 | `subscription_kernel`: subscription kernel is minimal: zero stored semantic rels, switchMap = keyed replace on an ordinary rel | ruling | user-ruled | measurement ('counting kill measured FLAT, 21 statements at cone depth 1..256') | v6/prolog/conformance/rulings.pl | L |
| R-028 | `spine_residency`: git/fs spine hosted in the language (stdlib rels+binds+salts), never kernel | ruling | user-ruled | none (user directive, no measurement) | v6/prolog/conformance/rulings.pl | L |
| R-029 | `clock_residency`: wall-clock cadence enters as a world-fed bind row, never a new construct | ruling | user-ruled | none (cites ghcacher F2 finding, not a measurement of this ruling itself) | v6/prolog/conformance/rulings.pl | M |
| R-030 | `lifecycle_arm_vocabulary`: lifecycle arm words = verbatim rx Observer vocabulary (next/finalize/unsubscribe/complete/subscribe/error); SQL trigger family rejected | ruling | user-ruled | none (user overruled the match-frontier lab's own SQL-trigger-family recommendation) | v6/prolog/conformance/rulings.pl | M |
| R-031 | `match_block_word`: the block word for arm dispatch is `match`, not partition/groupBy | ruling | user-ruled | none (user overruled the lab's own partition/groupBy pricing) | v6/prolog/conformance/rulings.pl | S |
| R-032 | `transition_rule_semantics`: boundary-collapsed transitions are first-to-last with mandatory collapse logging | ruling | user-ruled | none (user accepted the match-frontier lab's C2 crack as semantics, added a logging obligation) | v6/prolog/conformance/rulings.pl | M |
| R-033 | `rel_default_policy`: a bare rel is `value, unkeyed`; entity remains the marked case | ruling | user-ruled | none (overrides the round-2 types lab's 'no implicit policy' amendment) | v6/prolog/conformance/rulings.pl | L |
| R-034 | `enum_variant_separator`: enum variant separator is prolog's own semicolon | ruling | user-ruled | none (user quote, no measurement) | v6/prolog/conformance/rulings.pl | S |
| R-035 | `decl_column_spelling`: decl columns are `name(col: type, ...)`, colon-typed, source order significant; kills Key()/Min() wrappers | ruling | user-ruled | none (user quote, no measurement; wave-2 migration (53 kind(Ref,set) deletions, 49 files) is downstream execution, not evidence for the ruling itself) | v6/prolog/conformance/rulings.pl | L |
| R-036 | `enum_decl_in_rel`: enum variants live in the rel decl as prolog functors with the semicolon separator | ruling | user-ruled | none ('on the lowering argument', cites the types-lab enum-shape slot generally, no fresh measurement) | v6/prolog/conformance/rulings.pl | M |
| R-037 | `no_policy_suffix_words`: no policy suffix words on decls; `set` removed, `log` is the one kind word | ruling | user-ruled | none (user quote citing the types-lab verdict's own 'optional sugar' line) | v6/prolog/conformance/rulings.pl | M |
| R-038 | `edb_definition`: EDB is defined by absence: a never-headed rel is pure subject, no decl word marks it | ruling | user-ruled | none (user quote; reclassifies the binds-arc __lit_0 finding as a defect) | v6/prolog/conformance/rulings.pl | M |
| R-039 | `host_residency`: rows stay out of host (TS) residency; host sees deltas/aggregates, never a materialized table | ruling | user-ruled | none (user quote naming the scale-bench 10x gap and s3 OOM as the named suspects, not yet measured as caused by this) | v6/prolog/conformance/rulings.pl | L |
| R-040 | `expression_residency`: comparisons/arithmetic/string expressions fuse into emitted SQL; TS deopt only where sqlite lacks the function | ruling | user-ruled | none (user quote, reasoning only) | v6/prolog/conformance/rulings.pl | M |
| R-041 | `json_ticklog_encoding`: tick log renders json values as canonical JSON text, not prolog cons-term text | ruling | user-ruled | none (user chose from a multiple-choice round, no measurement in the ruling itself (regrade arc executed the consequence)) | v6/prolog/conformance/rulings.pl | M |
| R-042 | `udf_residency`: stay on @libsql for UDFs; core-SQL fusion + TS deopt over delta rows only, driver swap deferred | ruling | user-ruled | measurement (udf-graft lab empirically proved @libsql 0.17.4 has no UDF registration API) | v6/prolog/conformance/rulings.pl | M |
| R-043 | `keyed_level_head`: keyed() on a level-rule head is a compile error, not silent inert accumulation | ruling | user-ruled | none (user chose 'Compile error' from a multiple-choice round) | v6/prolog/conformance/rulings.pl | S |
| R-044 | `retention_count_lowering`: keep(count(N)) is lowered for real as a retracting rule over the log | ruling | user-ruled | none (user chose 'Lower it for real' from a multiple-choice round) | v6/prolog/conformance/rulings.pl | M |
| R-045 | `compound_storage`: struct/compound columns store as rel rows referenced by content id (struct-as-rows), never inline blobs | ruling | user-ruled | measurement (executes the types-as-rels lab design; lab receipts (rendered_text_stable_under_both_policies etc.) adopted, not relitigated) | v6/prolog/conformance/rulings.pl | L |
| R-046 | `watcher_dep`: stay on node fs.watch behind IWatchSource; @parcel/watcher only on a measured bench regression | ruling | user-ruled | none (user quote, deliberately deferred until a bench regression is measured) | v6/prolog/conformance/rulings.pl | S |
| R-047 | `struct_arrival_key_order`: struct arrival key order is insignificant; oracle canonicalizes at load from the decl | ruling | user-ruled | none (user quote, no measurement) | v6/prolog/conformance/rulings.pl | S |
| R-048 | `bool_column_type`: bool becomes a real 2VL column type, overruling the earlier row-presence/enum golden-plan shape | ruling | user-ruled | none (user quote, ergonomics argument only) | v6/prolog/conformance/rulings.pl | M |
| R-049 | `numeric_precision`: float/REAL + avg() approved; precision spelling designed inside the phase-5 arc | ruling | user-ruled | none (user quote, approval only) | v6/prolog/conformance/rulings.pl | M |
| R-050 | `json_key_hole_marker`: a json key-position hole is spelled `$name`, matching the value-position hole | ruling | user-ruled | measurement (json_syntax lab L3 already proved json_each(key,value) lowering with zero new SQL) | v6/prolog/conformance/rulings.pl | S |
| R-051 | `match_arm_tokens`: the `\|->`/`\|+>` match arm token pair is ratified, left-to-right reading order is the stated reason | ruling | user-ruled | none (user quote; 23 migrated fixtures are downstream execution, not evidence for the token choice itself) | v6/prolog/conformance/rulings.pl | S |
| R-052 | `json5_subset`: json5 subset = unquoted keys only, no trailing commas, no # comments | ruling | user-ruled | none (user quote) | v6/prolog/conformance/rulings.pl | S |
| R-053 | `list_spelling`: list type spelling is `list(type)` | ruling | user-ruled | none (user quote) | v6/prolog/conformance/rulings.pl | S |
| R-054 | `string_quote`: string literals parse under both quote styles | ruling | user-ruled | none (user quote) | v6/prolog/conformance/rulings.pl | S |
| R-055 | `descent_depth_cap`: `**` descent stays uncapped, like the CSS descendant combinator | ruling | user-ruled | none (user quote, reversibility argument (cap addable later, not removable)) | v6/prolog/conformance/rulings.pl | S |
| R-056 | `json_pattern_goal_spelling`: decode/2 named body atom chosen over a `body = {..}` operator on migration-cost grounds | ruling | coordinator | none (user delegated to 'whatever is easiest to change later'; coordinator's own reasoning, not measured) | v6/prolog/conformance/rulings.pl | S |
| R-057 | `scan_surface`: no new surface for scan-shaped programs; canonical spelling is keyed accumulator + log + match-block arms | ruling | user-ruled | none (user quote, deferred sugar until repetition shows the ugliness) | v6/prolog/conformance/rulings.pl | S |
| R-058 | `openapi_spec_artifact`: the generated OpenAPI spec is a checked-in artifact with a staleness gate | ruling | user-ruled | none (user quote) | v6/prolog/conformance/rulings.pl | S |
| R-059 | `openapi_route_list_generated`: the route list is generated from facts, not hand-kept twice | ruling | user-ruled | none (user quote) | v6/prolog/conformance/rulings.pl | M |
| R-060 | `openapi_generated_code_checked_in`: both spec and generated code are checked in | ruling | user-ruled | none (user quote) | v6/prolog/conformance/rulings.pl | S |
| R-061 | `null_design`: null never enters storage/type system; absence stays row-absence, get_else/2 spells a default at the use site | ruling | user-ruled | measurement (option-versus-null lab measured 4 candidates; candidate B (nullable columns) proven dead) | v6/prolog/conformance/rulings.pl | L |
| R-062 | `stream_ordinal_spelling`: stream ordinal = seq(name) column-type sugar; engine-minted @ binding is dead | ruling | user-ruled | none (user quote ('i HATE the @ symbol')) | v6/prolog/conformance/rulings.pl | S |
| R-063 | `zip_reserved_row`: deleting the reserved `zip` row would make a typo a silent empty EDB; keep the refusal, name the equijoin in the message | ruling | user-ruled | none (user quote ('do the least fucky thing')) | v6/prolog/conformance/rulings.pl | S |
| R-064 | `stream_backpressure`: backpressure = watermark-gated writer, visible overflow rel, zero new constructs | ruling | user-ruled | none (user quote; CSP/clock follow-up banked as a future arc, not built) | v6/prolog/conformance/rulings.pl | M |
| R-065 | `latest_over_log`: latest() over a Log rel refuses at load, naming the max(Ordinal) rewrite | ruling | user-ruled | none (card 5b reasoning only) | v6/prolog/conformance/rulings.pl | S |
| R-066 | `stream_decl_word`: no dedicated 'stream' decl word; log+ordinal+keep already state the definition | ruling | user-ruled | none (card 6a reasoning only) | v6/prolog/conformance/rulings.pl | S |
| R-067 | `cross_rel_drain_order`: cross-rel delta interleaving is a documented non-contract, not fixed behavior | ruling | user-ruled | measurement ('measured not-fixable-in-general by the runtime bridge arc') | v6/prolog/conformance/rulings.pl | M |
| R-068 | `json_null_token`: json null = a reserved ground compound term, never a bare atom | ruling | user-ruled | none (user proposed (), reasoned into a compound to avoid atom-collision with text values) | v6/prolog/conformance/rulings.pl | S |
| R-069 | `json_dup_key_fate`: emitter refuses on json duplicate keys, matching the oracle's existing throw | ruling | user-ruled | none (user quote ('emitter throws if oracle throws')) | v6/prolog/conformance/rulings.pl | S |
| R-070 | `vocabulary_tiebreak`: naming ties break toward SQLite spelling first, then ANSI SQL; rx/prolog words only where no storage-plane spelling exists | ruling | user-ruled | none (user quote; does not itself trigger the B8 non-SQL word renames) | v6/prolog/conformance/rulings.pl | S |
| R-071 | `seq_sugar`: seq sugar approved, M2 (cursor numbering) only; M1 scan and M3 stages stay unwired | ruling | user-ruled | none (user: 'approve seq') | v6/prolog/conformance/rulings.pl | M |
| R-072 | `release_gate_v620`: v6.2.0 push/tag gated on ARCH-MAP.md generated from a single dl6 file (python renderer must go) | ruling | user-ruled | none (user quote, gate condition not evidence) | v6/prolog/conformance/rulings.pl | S |
| R-073 | `devlog_rail`: approve a dl6 program that reads session ledgers and emits DEVLOG.md | ruling | user-ruled | none (user quote ('docs YES DOGFOOD DOCS')) | v6/prolog/conformance/rulings.pl | S |
| R-074 | `glob_dialect`: both watch-boot and live halves use node's path.matchesGlob dialect, agreeing with v5 globset | ruling | user-ruled | measurement (fixes a measured 170/242 corpus disagreement (glob_dialect_split)) | v6/prolog/conformance/rulings.pl | M |
| R-075 | `bench_reference`: big-scale reference engine (tsv2 first) earns reference status by byte-proof vs the swipl oracle across the whole reachable corpus | ruling | user-ruled | none (user quote, sets a future measurement bar rather than citing one) | v6/prolog/conformance/rulings.pl | M |
| R-076 | `type_gate_widening`: decl-type arrival refusal gate widens to all column types/positions; coercion follows SQLite affinity | ruling | user-ruled | none (user quote ('do what sql would do')) | v6/prolog/conformance/rulings.pl | L |
| R-077 | `wide_int_fate`: integers beyond 2^53-1 refuse everywhere (named int_out_of_range) with a TODO marking a future bigint door | ruling | user-ruled | none (user quote, explicit deferral) | v6/prolog/conformance/rulings.pl | S |
| R-078 | `files_naming`: file enumeration hosts are `files(glob,...)` (unmarked worktree) vs `files_at(rev, glob,...)` (marked pinned rev); word `scan` banned | ruling | user-ruled | none (user quote, consistent with the standing spine_residency ruling) | v6/prolog/conformance/rulings.pl | M |
| R-079 | `org_fanout`: repo list is an ordinary sh host on a 1-day clock bind, fan-out via ordinary joins | ruling | user-ruled | none (user quote, zero new constructs claimed but not measured here) | v6/prolog/conformance/rulings.pl | S |
| R-080 | `gen_word_banned`: the word `gen` is banned for the codegen-sink construct; naming must come from rx/prolog/SQL vocabulary | ruling | user-ruled | none (user quote ('gen needs a new name i hate the name gen')) | v6/prolog/conformance/rulings.pl | S |
| R-081 | `repo_column_spelling`: repo-scoped enumeration is its own host pair (repo_files/repo_files_at), never a required leading cwd-literal column | ruling | user-ruled | none (user quote, follows the repo_grep_at precedent) | v6/prolog/conformance/rulings.pl | M |
| N-001 | Refuse an aggregate whose GROUP BY key is not delta-local (would force a full-table recompute) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2183 | M |
| N-002 | Refuse aggregate heads outside the reserved count/sum/min/max/json_* set (occurrence found in a labs/ file, may be stale) | named-refusal | user-ruled | fixture (ruling q9_aggregate_heads reserves the head-position forms; ordered-aggregate lab shows `aggregate_head(json_array(_))` still refused at HEAD) | v6/prolog/labs/json_interop/0_receipts.pl:276 | M |
| N-003 | Compiler refuses `aggregate_head_mixed_with_plain_clause` (aggregate head mixed with plain clause) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1765 | S |
| N-004 | Compiler refuses `aggregate_head_no_positive_body` (aggregate head no positive body) | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1503 | S |
| N-005 | Compiler refuses `aggregate_head_reads_itself` (aggregate head reads itself) | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1499 | S |
| N-006 | Compiler refuses `aggregate_head_shape` (aggregate head shape) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:583 | S |
| N-007 | Compiler refuses `aggregate_in_edge_head` (aggregate in edge head) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:2524 | S |
| N-008 | Compiler refuses `aggregate_kind_not_lowered` (aggregate kind not lowered) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2890 | S |
| N-009 | Compiler refuses `aggregate_operand_not_number` (aggregate operand not number) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2913 | S |
| N-010 | Compiler refuses `aggregate_ordinal_not_int` (aggregate ordinal not int) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2900 | S |
| N-011 | Compiler refuses `aggregate_separator_not_constant` (aggregate separator not constant) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2906 | S |
| N-012 | Compiler refuses `arith_operand_not_int` (arith operand not int) | named-refusal | agent-verdict | none | v6/prolog/print_dl.pl:87 | S |
| N-013 | Compiler refuses `arith_operand_not_number` (arith operand not number) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:524 | S |
| N-014 | Compiler refuses `at` (at) | named-refusal | agent-verdict | none | v6/prolog/compile.pl:226 | S |
| N-015 | Compiler refuses `bind_mismatch` (bind mismatch) | named-refusal | agent-verdict | none | v6/prolog/1_host_expand.pl:390 | S |
| N-016 | Compiler refuses `coalesce_in_head` (coalesce in head) | named-refusal | agent-verdict | none | v6/prolog/0_coalesce_expand.pl:262 | S |
| N-017 | Compiler refuses `coalesce_multiple_outputs` (coalesce multiple outputs) | named-refusal | agent-verdict | none | v6/prolog/0_coalesce_expand.pl:161 | S |
| N-018 | Compiler refuses `coalesce_no_output` (coalesce no output) | named-refusal | agent-verdict | none | v6/prolog/0_coalesce_expand.pl:157 | S |
| N-019 | Compiler refuses `coalesce_not_top_level` (coalesce not top level) | named-refusal | agent-verdict | none | v6/prolog/0_coalesce_expand.pl:256 | S |
| N-020 | Compiler refuses `coalesce_output_not_column` (coalesce output not column) | named-refusal | agent-verdict | none | v6/prolog/0_coalesce_expand.pl:166 | S |
| N-021 | Compiler refuses `coalesce_source_not_rel_atom` (coalesce source not rel atom) | named-refusal | agent-verdict | none | v6/prolog/0_coalesce_expand.pl:149 | S |
| N-022 | Compiler refuses `column_mismatch` (column mismatch) | named-refusal | agent-verdict | none | v6/prolog/1_host_expand.pl:205 | S |
| N-023 | Compiler refuses `column_ref_type_conflict` (column ref type conflict) | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:771 | S |
| N-024 | Compiler refuses `column_type_unknown` (column type unknown) | named-refusal | agent-verdict | none | v6/prolog/0_type_plane.pl:128 | S |
| N-025 | Compiler refuses `comparison_type_mismatch` (comparison type mismatch) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:1059 | S |
| N-026 | Compiler refuses `concat_non_display_piece` (concat non display piece) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:565 | S |
| N-027 | Compiler refuses `concat_not_a_list` (concat not a list) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:556 | S |
| N-028 | Compiler refuses `decode_field_unknown` (decode field unknown) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1325 | S |
| N-029 | Compiler refuses `decode_pattern_not_object` (decode pattern not object) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1319 | S |
| N-030 | Compiler refuses `decode_source_not_bound` (decode source not bound) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2581 | S |
| N-031 | Compiler refuses `decode_source_not_struct` (decode source not struct) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1294 | S |
| N-032 | Compiler refuses `edge_body_multiple_finalize` (edge body multiple finalize) | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:850 | S |
| N-033 | Compiler refuses `edge_body_needs_negation` (edge body needs negation) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:702 | S |
| N-034 | Compiler refuses `edge_body_needs_now` (edge body needs now) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:751 | S |
| N-035 | Compiler refuses `edge_body_with_latest` (edge body with latest) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:883 | S |
| N-036 | Compiler refuses `edge_body_with_negation` (edge body with negation) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:723 | S |
| N-037 | Compiler refuses `edge_body_with_now` (edge body with now) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:764 | S |
| N-038 | Refuse an edge-rule head whose column type conflicts with its feeding body | named-refusal | agent-verdict | fixture (2 rev-pin/diag fixtures flipped by the expression+aggregate lift arc) | v6/prolog/analyze.pl:1036 | M |
| N-039 | Refuse an edge-rule head write judged at risk of conflicting with another writer of the same row | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1350 | M |
| N-040 | Refuse an edge-rule write into a Set rel with no key declared | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1545 | M |
| N-041 | Refuse an edge trigger atom that is not a Log rel where Log is required | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1477 | M |
| N-042 | Compiler refuses `enum_variant_column_shape` (enum variant column shape) | named-refusal | agent-verdict | none | v6/prolog/0_enum_expand.pl:143 | S |
| N-043 | Compiler refuses `enum_variant_name_collision` (enum variant name collision) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:1144 | S |
| N-044 | Compiler refuses `enum_variant_shape` (enum variant shape) | named-refusal | agent-verdict | none | v6/prolog/0_enum_expand.pl:134 | S |
| N-045 | Refuse a `finalize` lifecycle arm inside a level (`<-`) rule; lifecycle arms are edge-only | named-refusal | agent-verdict | fixture (agreement-test fixture, restored after a compiler regression let it through (tick-phase-alignment landing)) | v6/prolog/0_refusal_messages.pl:5 | M |
| N-046 | Refuse a guard-goal shape the compiler cannot lower | named-refusal | agent-verdict | none | v6/prolog/lower.pl:617 | S |
| N-047 | Compiler refuses `head_arithmetic` (head arithmetic) | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1284 | S |
| N-048 | Compiler refuses `head_expr` (head expr) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:453 | S |
| N-049 | Compiler refuses `host_executor_mismatch` (host executor mismatch) | named-refusal | agent-verdict | none | v6/prolog/1_host_expand.pl:198 | S |
| N-050 | Compiler refuses `int_out_of_range` (int out of range) | named-refusal | agent-verdict | none | v6/prolog/compile.pl:190 | S |
| N-051 | Refuse a join across columns with mismatched SQL type affinity | named-refusal | agent-verdict | measurement (Q4 reconciliation caught a real cross-type-join miscompile ('1' vs 1)) | v6/prolog/compile/test/plunit_tests.pl:1070 | M |
| N-052 | Compiler refuses `json_capture_type_unknown` (json capture type unknown) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2725 | S |
| N-053 | Compiler refuses `json_key_contains_quote` (json key contains quote) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2621 | S |
| N-054 | Compiler refuses `json_key_shape` (json key shape) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2787 | S |
| N-055 | Compiler refuses `json_pattern_shape` (json pattern shape) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2709 | S |
| N-056 | Compiler refuses `json_value_expression` (json value expression) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:443 | S |
| N-057 | Refuse `keep(...)` retention clause on a non-Log rel | named-refusal | agent-verdict | fixture (fail-first fixture, org-refactor 'theorem six') | v6/prolog/compile/test/plunit_tests.pl:950 | S |
| N-058 | Compiler refuses `key_position_duplicate` (key position duplicate) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:2324 | S |
| N-059 | Compiler refuses `key_position_out_of_range` (key position out of range) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:2316 | S |
| N-060 | Compiler refuses `keyed_conflict` (keyed conflict) | named-refusal | agent-verdict | none | v6/prolog/conformance/engine.pl:363 | S |
| N-061 | Refuse `keyed()` on a level-rule head as a compile error (was silently inert before) | named-refusal | user-ruled | fixture (ruling `keyed_level_head`; hands-on findings inert-accumulation receipt) | v6/prolog/compile/test/plunit_tests.pl:1207 | S |
| N-062 | Refuse `keyed()` combined with a Log rel kind | named-refusal | agent-verdict | fixture (plunit fixture) | v6/prolog/compile/test/plunit_tests.pl:2342 | S |
| N-063 | Refuse `latest()` sampling inside a level-rule body | named-refusal | agent-verdict | fixture (review-B2 fail-first refusal fixture; edge-body latest() later landed separately) | v6/prolog/compile/test/plunit_tests.pl:940 | M |
| N-064 | Refuse a level-rule body goal shape the compiler does not recognize | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1449 | S |
| N-065 | Refuse a level rule with no positive body atom | named-refusal | agent-verdict | none | v6/prolog/lower.pl:2477 | M |
| N-066 | Reserve rx lifecycle words (next/finalize/subscribe/unsubscribe/complete/error) from redefinition as ordinary rel/rule names | named-refusal | user-ruled | none (ruling lifecycle_arm_vocabulary) | v6/prolog/0_program_check.pl:400 | M |
| N-067 | Compiler refuses `list_element_not_scalar` (list element not scalar) | named-refusal | agent-verdict | none | v6/prolog/0_type_plane.pl:121 | S |
| N-068 | Compiler refuses `list_of_relation_refs` (list of relation refs) | named-refusal | agent-verdict | none | v6/prolog/0_type_plane.pl:120 | S |
| N-069 | Refuse declaring `log` rel-kind on a rel that a level rule heads | named-refusal | agent-verdict | fixture (review-B2 fail-first refusal fixture) | v6/prolog/compile/test/plunit_tests.pl:933 | M |
| N-070 | Compiler refuses `match_arm_head_not_positive_rel` (match arm head not positive rel) | named-refusal | agent-verdict | none | v6/prolog/0_match_expand.pl:105 | S |
| N-071 | Compiler refuses `match_arm_shape` (match arm shape) | named-refusal | agent-verdict | none | v6/prolog/0_match_expand.pl:100 | S |
| N-072 | Refuse a `match` block whose arms do not cover every enum variant | named-refusal | agent-verdict | fixture (enum coverage checked, match+rulings lane fixture) | v6/prolog/compile/test/plunit_tests.pl:1195 | S |
| N-073 | Compiler refuses `match_source_not_positive_rel` (match source not positive rel) | named-refusal | agent-verdict | none | v6/prolog/0_match_expand.pl:51 | S |
| N-074 | Refuse a Log rel declared without the required `keep` retention clause | named-refusal | user-ruled | fixture (ruling q10_retention requires keep on Log rels; plunit fixture) | v6/prolog/compile/test/plunit_tests.pl:2515 | S |
| N-075 | Compiler refuses `negated_guard_goal` (negated guard goal) | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1463 | S |
| N-076 | Compiler refuses `non_finite_float_literal` (non finite float literal) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:204 | S |
| N-077 | Refuse `now/1` tick-counter read inside a level-rule body | named-refusal | agent-verdict | none | v6/prolog/analyze.pl:1460 | M |
| N-078 | Compiler refuses `openapi_type_unknown` (openapi type unknown) | named-refusal | agent-verdict | none | v6/prolog/labs/openapi_codegen/emit_openapi.pl:210 | S |
| N-079 | Compiler refuses `oracle_refuses_live_capture_type` (oracle refuses live capture type) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:3391 | S |
| N-080 | Compiler refuses `param_count_mismatch` (param count mismatch) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/run_sql_check.pl:350 | S |
| N-081 | Compiler refuses `pattern_arg` (pattern arg) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:251 | S |
| N-082 | Refuse `pre` chained read inside a level-rule body | named-refusal | agent-verdict | fixture (review-B2 fail-first refusal fixture) | v6/prolog/compile/test/plunit_tests.pl:911 | M |
| N-083 | Compiler refuses `probe_mismatch` (probe mismatch) | named-refusal | agent-verdict | none | v6/prolog/print_dl.pl:432 | S |
| N-084 | Compiler refuses `query_mismatch` (query mismatch) | named-refusal | agent-verdict | none | v6/prolog/1_host_expand.pl:412 | S |
| N-085 | Compiler refuses `quote_in_literal` (quote in literal) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:207 | S |
| N-086 | Refuse a rule graph whose level-rule dependency forms a cycle (non-stratifiable negation/recursion) | named-refusal | agent-verdict | none (matches the standing not_stratified guard; tabling-verdict lab confirms this IS semantics, not an artifact) | v6/prolog/strat.pl:98 | L |
| N-087 | Compiler refuses `seq_cursor_name_collision` (seq cursor name collision) | named-refusal | agent-verdict | none | v6/prolog/0_seq_expand.pl:134 | S |
| N-088 | Compiler refuses `seq_in_level_rule` (seq in level rule) | named-refusal | agent-verdict | none | v6/prolog/0_seq_expand.pl:37 | S |
| N-089 | Compiler refuses `seq_partition_type_unknown` (seq partition type unknown) | named-refusal | agent-verdict | none | v6/prolog/0_seq_expand.pl:190 | S |
| N-090 | Remove `set` as a decl suffix word; a bare rel is already a set table | named-refusal | user-ruled | none (ruling no_policy_suffix_words) | v6/prolog/compile/parse_dl.pl:572 | M |
| N-091 | Compiler refuses `sql_text_mismatch` (sql text mismatch) | named-refusal | agent-verdict | none | v6/prolog/labs/json_syntax/2_lowering.pl:375 | S |
| N-092 | Compiler refuses `surface_findings` (surface findings) | named-refusal | agent-verdict | none | v6/prolog/compile.pl:203 | S |
| N-093 | Compiler refuses `tagged_brace_reserved` (tagged brace reserved) | named-refusal | agent-verdict | fixture (dedicated plunit regression test) | v6/prolog/compile/test/plunit_tests.pl:3311 | S |
| N-094 | Compiler refuses `template_mismatch` (template mismatch) | named-refusal | agent-verdict | none | v6/prolog/1_host_expand.pl:291 | S |
| N-095 | Compiler refuses `text_operand_not_text` (text operand not text) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:474 | S |
| N-096 | Compiler refuses `trigger_arg_not_var` (trigger arg not var) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:1708 | S |
| N-097 | Compiler refuses `type_arrival_shape_mismatch` (type arrival shape mismatch) | named-refusal | agent-verdict | none | v6/prolog/conformance/engine.pl:535 | S |
| N-098 | Compiler refuses `unbound_head_var` (unbound head var) | named-refusal | agent-verdict | none | v6/prolog/ARCH.pl:714 | S |
| N-099 | Compiler refuses `unknown_comparison_operator` (unknown comparison operator) | named-refusal | agent-verdict | none | v6/prolog/lower.pl:646 | S |
| N-100 | Compiler refuses `value_template_never_shipped` (value template never shipped) | named-refusal | agent-verdict | none | v6/prolog/labs/json_syntax/2_lowering.pl:102 | S |
| N-101 | Reserve `zip` as a construct name; refuse redefining it as an ordinary rel/rule name | named-refusal | user-ruled | none (ruling zip_reserved_row) | v6/prolog/0_program_check.pl:400 | S |
| V-001 | N+1 scream threshold NOT raised after relabeling bump keys (a real per-row leak still screams) | semantic-refusal | agent-verdict | measurement (real corpus attribution table, 143 flushes reclassified, 0 plain leak) | plans/2026-07-20-strings-n1-verdict.md | S |
| V-002 | Per-family batching of encode_rel_rows DROPPED (140->136 flushes, marginal, extra code for no scream-silencing benefit) | library-rejection | agent-verdict | measurement (measured 140->136 on the sprefa corpus) | plans/2026-07-20-strings-n1-verdict.md | S |
| V-003 | Dataflow node identity candidate (A) dense sequence with no dictionary REJECTED (crash-resumed slice would mint different ids) | ruling | agent-verdict | none (reasoning only, 'ruled out exactly as the mandate anticipated') | plans/2026-07-20-strings-n1-verdict.md | M |
| V-004 | Dataflow node identity candidate (B) composite key (file,line,col,kind on every id column) REJECTED on blast radius | ruling | user-ruled | measurement (df_edge 2->8 cols, ~243 df tests, user's own note '8 cols vs 2 and I think worse') | plans/2026-07-20-strings-n1-verdict.md | L |
| V-005 | Dataflow node identity candidate (C) dense surrogate via persistent _df_node_dict CHOSEN | ruling | agent-verdict | fixture (629 lib/966 it green, 0 orphan refs measured on 282,109 nodes) | plans/2026-07-20-strings-n1-verdict.md | L |
| V-006 | AsyncLocalStorage rejected for trace context propagation; explicit tick/rel args used instead | library-rejection | agent-verdict | none (cites external ALS overhead benchmarks (4-12%), not measured in this repo) | plans/2026-07-27-perf-tracing-buy-verdict.md | M |
| V-007 | node:diagnostics_channel tracingChannel + pino CHOSEN as the tracing spine/emit layer over OpenTelemetry JS and a hand-rolled fs.appendFile writer | library-rejection | user-ruled | none (user approved the one new dep (pino) per the ledger; doc itself just proposes) | plans/2026-07-27-perf-tracing-buy-verdict.md | M |
| V-008 | OpenTelemetry JS deferred as an escalation path, not rejected outright (becomes right only when a cross-runner schema is needed) | library-rejection | agent-verdict | none (reasoning only) | plans/2026-07-27-perf-tracing-buy-verdict.md | S |
| V-009 | SWI tabling rejected as a replacement for the hand-rolled fixpoint engine: SHIFTS SEMANTICS on stratified negation (no not_stratified guard under tabling) | semantic-refusal | agent-verdict | measurement (100/100 fixtures byte-identical + 1 adversarial tripwire diverges (tabled version wrongly derives both p and q)) | plans/2026-07-27-tabling-verdict.md | L |
| V-010 | Pacing (a), landing a whole queue in one drain tick, rejected as a queue implementation (loses N-1 of N items at a keyed consumer) | semantic-refusal | agent-verdict | measurement (round 2 receipt: 2 of 3 items vanish, survivor picked by column order not ordinal) | plans/2026-07-28-consumption-arms-verdict.md | M |
| V-011 | Error arm as a second failure channel REFUSED; error arm modeled as an ordinary enum-variant destructure over the Log envelope instead | semantic-refusal | agent-verdict | measurement ('THE ERROR-ARM RESOLUTION' section, three named costs weighed) | plans/2026-07-28-consumption-arms-verdict.md | M |
| V-012 | Pacing (b), one-per-drain-tick, PROPOSED as the queue-pacing spelling (SLOT-QUEUE-PACING); not yet user-ratified at doc time | semantic-refusal | unclear | measurement (lab proposal only, marked 'PROPOSED (b), with the drain-cap cost named' in the slot table) | plans/2026-07-28-consumption-arms-verdict.md | M |
| V-013 | Semi-naive delta join statement family: MIXED spelling (join/projection text inline, statement execution via shared helper) | ruling | agent-verdict | fixture (byte-identity across both spellings, 8 stmts/tick both) | plans/2026-07-28-emitter-p0-lab-verdict.md | S |
| V-014 | Count-IVM support maintenance: HELPER spelling chosen (42 vs 7 lines still helper-favored) | ruling | agent-verdict | fixture (byte-identity 292/292 bytes, 15 stmts/tick both) | plans/2026-07-28-emitter-p0-lab-verdict.md | S |
| V-015 | DISTINCT placement: MIXED spelling (keyword inline in specialized SQL, execution shared) | ruling | agent-verdict | fixture (byte-identity across both fixtures) | plans/2026-07-28-emitter-p0-lab-verdict.md | S |
| V-016 | Boundary-diff-from-delta-stream: HELPER spelling chosen (zero full-table snapshot scans either way) | ruling | agent-verdict | fixture (byte-identity 292/292, 12 stmts/tick, zero full-table reads) | plans/2026-07-28-emitter-p0-lab-verdict.md | S |
| V-017 | Drain overflow ANSWERED as error, never silent spill to a Ta channel (SLOT-SPILL) | semantic-refusal | agent-verdict | measurement (scenario b3 receipt: spilling trades loud failure for silent loss) | plans/2026-07-28-match-frontier-lab-verdict.md | M |
| V-018 | Async/Ta marker DISSOLVES entirely; no marked-vs-unmarked distinction needed for async firing (SLOT-TA-MARK) | semantic-refusal | agent-verdict | measurement (scenarios f1-f4, DIRECT-BUT-VACUOUS lowering) | plans/2026-07-28-match-frontier-lab-verdict.md | M |
| V-019 | Two-axis nesting order (match block over transitions) ANSWERED as not forced (SLOT-NEST) | ruling | agent-verdict | measurement (scenario e1) | plans/2026-07-28-match-frontier-lab-verdict.md | S |
| V-020 | SQL trigger family (inserted/deleted/OLD/NEW) priced as the update-arm spelling, then user-overruled in favor of rx Observer words | semantic-refusal | user-ruled | none (superseded by ruling lifecycle_arm_vocabulary) | plans/2026-07-28-match-frontier-lab-verdict.md | S |
| V-021 | FK ON DELETE CASCADE rejected as a retraction strategy: wrong on shared children (arbitrary owner picked, dangling refs left, no error) and hard-fails past sqlite trigger_depth 1000 | semantic-refusal | agent-verdict | measurement (20/20 matrix vs real sqlite3; 1001-node chain rejected by sqlite itself) | plans/2026-07-28-sqlite-retraction-verdict.md | L |
| V-022 | support_count rejected as a general retraction strategy: wrong on cycles (count never reaches zero) and 9999 rounds/19s at a 10k chain | semantic-refusal | agent-verdict | measurement (10k-row timing table, cycle scenario both rows survive full release) | plans/2026-07-28-sqlite-retraction-verdict.md | M |
| V-023 | Recursive-CTE fixpoint reseed CHOSEN as the retraction strategy: correct on DAGs and cycles, 9ms at 10k, no depth ceiling | ruling | agent-verdict | measurement (2-3 orders of magnitude cheaper than support_count at 10k rows) | plans/2026-07-28-sqlite-retraction-verdict.md | L |
| V-024 | FK ON DELETE CASCADE ruled out for the value plane a second time (finding 6): decisively wrong plus no rx lowering exists | semantic-refusal | agent-verdict | measurement (shared child survives support 2->1 correctly; cascade deletes + leaves dangling refs) | plans/2026-07-28-types-as-rels-verdict.md | L |
| V-025 | Content ids cannot express cyclic graphs; cyclic structures need extrinsic entity keys, not content-hash identity | ruling | agent-verdict | measurement (parent-id-derives-from-child-id argument, domination dissolves into support counting on DAGs only) | plans/2026-07-28-types-as-rels-verdict.md | L |
| V-026 | Struct/enum spelling (b) prolog functors CHOSEN over (c) plain rels and (a) json braces | ruling | agent-verdict | none ('criteria visible, no fiat', worked example only, no measurement cited) | plans/2026-07-28-types-as-rels-verdict.md | M |
| V-027 | := assignment REJECTED as a new construct; it is sugar for naming a column on an undeclared head rel, byte-identical to the argument-position spelling | semantic-refusal | agent-verdict | measurement (30 real use sites census, byte-identical emitted TypeScript for both spellings) | plans/2026-07-29-assign-composition-verdict.md | S |
| V-028 | Text-manipulation operators (57 v5 call sites: =~/replace_re/trim/split/json/match_line/match_ast) do NOT come back as language constructs; all move into host templates | semantic-refusal | agent-verdict | measurement (745/745 byte-for-byte parity with v5 comment_node achieved without them) | plans/2026-07-29-comment-node-verdict.md | M |
| V-029 | Route b2 (host template pre-filters to comments before crossing the rel boundary) chosen over full extractor grammar growth (a), whole-CST-stream crossing (b1), or grep-only (c, not string-safe) | library-rejection | agent-verdict | measurement (rows-crossing/bytes/wall-ms table across 4 routes) | plans/2026-07-29-comment-node-verdict.md | M |
| V-030 | Zero-decl rel-name bind activation REFUSED as a magic-rel hazard; `bind interval(...)` selected instead | semantic-refusal | agent-verdict | none (reasoning only, cites the v5 magic-rel ban) | plans/2026-07-29-hosts-extraction-verdict.md | M |
| V-031 | ts_query coercion of sg_pattern REFUSED; sg_pattern kept as its own construct family (slot_sg_metavariable_semantics) | semantic-refusal | agent-verdict | measurement (12/12 ts_query features mapped cleanly; sg metavariables did not fit that mapping) | plans/2026-07-29-hosts-extraction-verdict.md | M |
| V-032 | Spread spelling `rel b(...a, extra: int)` CHOSEN over an `include a` declaration word (splice position changes the positional program; include cannot state it) | ruling | agent-verdict | measurement (worked cross-language receipts (real tsc/rustc/go) in the verdict) | plans/2026-07-29-rel-spreading-verdict.md | M |
| V-033 | Width subtyping on spread targets REFUSED in both directions (arity must match exactly) | semantic-refusal | agent-verdict | measurement (54 PASS lab receipts) | plans/2026-07-29-rel-spreading-verdict.md | M |
| V-034 | Plane/key modifiers (keyed, keep) NEVER travel through a spread; inheriting keyed is refused, inheriting keep is invisible to tick-log grading | semantic-refusal | agent-verdict | measurement (the retention-grading gap class again, per the lab's own framing) | plans/2026-07-29-rel-spreading-verdict.md | L |
| V-035 | Derived rels REFUSED as spread sources (spread_source_not_declared); only generated (enum-expanded) or plain decl sources allowed | semantic-refusal | agent-verdict | none (reasoning: 'the real line is GENERATED vs INFERRED') | plans/2026-07-29-rel-spreading-verdict.md | M |
| V-036 | node-sqlite3 UDF path rejected: fails to load on node 24 arm64 | semantic-refusal | agent-verdict | measurement (named slot NODE_SQLITE3_ABI, load failure reproduced) | plans/2026-07-29-sqlite-udf-graft-verdict.md | S |
| V-037 | Rust sidecar UDF registration proven working but deferred to 'the rust return' | library-rejection | agent-verdict | measurement (proof-of-concept registration proven, not adopted now) | plans/2026-07-29-sqlite-udf-graft-verdict.md | M |
| V-038 | OLD/NEW update arm needs no new construct: `changed(K,Old,New) <+ finalize(r(K,Old)), r(K,New)`, mandatory EDGE arrow; level spelling refused at load | ruling | agent-verdict | measurement (19 PASS lab receipts, fires replace-tick plus one) | plans/2026-07-29-update-arm-verdict.md | M |
| V-039 | Same-tick v1->v2->v3 collapses to ONE (v1,v3) row (U4); settles match-frontier C2 as net-transition-per-tick semantics | ruling | agent-verdict | measurement (U4 receipt) | plans/2026-07-29-update-arm-verdict.md | M |
| V-040 | finalize over a Log rel is silently dead with no refusal (U5); load-time refusal RECOMMENDED, not yet built (SLOT-LOG-FINALIZE-REFUSAL) | semantic-refusal | unclear | measurement (recommendation only, decidable per the doc, not executed at verdict time) | plans/2026-07-29-update-arm-verdict.md | S |
| V-041 | seq(name) cursor-numbering sugar WIDENED based on measured 78% verbatim-shape rule repetition in CSP idioms (52/94 rules share one queue template) | ruling | agent-verdict | measurement (73 of 94 rules verbatim-shape repeats, single template accounts for 52) | plans/2026-07-30-csp-idioms-verdict.md | M |
| V-042 | avro .avsc schema format: BOUGHT at zero cost (already JSON, no parser needed) | library-rejection | agent-verdict | measurement (vendored Apache interop.avsc validated) | plans/2026-07-30-extract-t2-verdict.md | S |
| V-043 | protoc --descriptor_set_out REJECTED for JSON schema extraction (binary FileDescriptorSet, no JSON flag); protobufjs pbjs -t json BOUGHT instead | library-rejection | agent-verdict | measurement (protoc --help checked directly, 4 descriptors generated via pbjs incl protoc's own 58,877-byte descriptor.proto) | plans/2026-07-30-extract-t2-verdict.md | S |
| V-044 | buf CLI JSON descriptor output priced but NOT verified/adopted (not installed, documentation-only row) | library-rejection | agent-verdict | none (explicitly marked 'not verified' in the doc's own table) | plans/2026-07-30-extract-t2-verdict.md | S |
| V-045 | No column type is correct for a decode/2 hole over a heterogeneous schema slot (string-or-object, e.g. Avro `type`); text renders wrong on one door, json silently drops scalars (finding D2) | semantic-refusal | agent-verdict | measurement (cross-door byte-diff on real vendored schemas) | plans/2026-07-30-extract-t2-verdict.md | L |
| V-046 | nst/JSONTestSuite BOUGHT as the json-parsing accept/reject reference corpus | library-rejection | agent-verdict | measurement (318 test_parsing files, y_/n_/i_ classification used directly) | plans/2026-07-30-json-flex-verdict.md | S |
| V-047 | JSON_checker (Crockford) REJECTED: strict subset of JSONTestSuite, no license file | library-rejection | agent-verdict | none (comparison against JSONTestSuite coverage) | plans/2026-07-30-json-flex-verdict.md | S |
| V-048 | google/json-test-suite REJECTED: no per-case accept/reject classification | library-rejection | agent-verdict | none (reasoning only) | plans/2026-07-30-json-flex-verdict.md | S |
| V-049 | fast-check/jsverify REJECTED as a dependency, its round-trip SHAPE adopted as a fixed 136-value enumeration instead of seeded random generation | library-rejection | agent-verdict | none (reasoning: cross-target contract receipts must be reproducible/diffable) | plans/2026-07-30-json-flex-verdict.md | S |
| V-050 | Bespoke json test-document generator REFUSED (JSONTestSuite already fits and is what other implementations report against) | library-rejection | agent-verdict | none (reasoning only) | plans/2026-07-30-json-flex-verdict.md | S |
| V-051 | One aggregate spelling cannot select both value-sort and stream-order semantics; two ORDER BY sources need two aggregate call sites (slot_order_axis) | semantic-refusal | agent-verdict | measurement (SQL probe against sqlite 3.45.1, both spellings tried) | plans/2026-07-30-ordered-aggregate-verdict.md | M |
| V-052 | group_concat given its OWN aggregate spelling rather than reusing json_group_array + a JSON-parse step (slot_string_join_spelling) | ruling | agent-verdict | measurement (sqlite accepts inner ORDER BY directly) | plans/2026-07-30-ordered-aggregate-verdict.md | S |
| V-053 | M1 scan-fold sugar AMENDED to weakest of three moves: zero real fold usage found outside cursors across the whole CSP corpus, and it lengthens a two-trigger rel | semantic-refusal | agent-verdict | measurement (18 of 18 pre reads are cursors; receipt 4d shows 2->4 rules on a dual-trigger rel) | plans/2026-07-30-point-free-verdict.md | S |
| V-054 | M3 \|> pipe sugar CONFIRMED for level rules, REFUSED for edge rules (wrong two different ways, one loud one silent) | semantic-refusal | agent-verdict | measurement (receipts 4b, 4c) | plans/2026-07-30-point-free-verdict.md | M |
| V-055 | graphpl@0.1.1 SWI pack REJECTED (0.1.x third-party dependency, does not even cover SCC, would be the toolchain's first pack dependency) | library-rejection | agent-verdict | measurement (pack_list('scc',...) returned no matching packages) | plans/2026-07-30-prolog-graph-buy-verdict.md | S |
| V-056 | library(ugraphs) BOUGHT for representation/transpose/topological-sort/cycle-detection over hand-written code or SWI tabling | library-rejection | agent-verdict | measurement (9-shape benchmark table, all linear, zero deps) | plans/2026-07-30-prolog-graph-buy-verdict.md | S |
| V-057 | SCC via ugraphs' Warshall-composed transitive_closure/2 REJECTED for the component engine (27,082ms at 1000 nodes) | semantic-refusal | agent-verdict | measurement (chain1000 benchmark row) | plans/2026-07-30-prolog-graph-buy-verdict.md | S |
| V-058 | Hand-written Kosaraju CHOSEN for SCC decomposition, written over ugraphs' own transpose_ugraph/neighbour structure (27ms at 1000 nodes, ~40 lines) | ruling | agent-verdict | measurement (same benchmark table, hand Tarjan/Kosaraju column) | plans/2026-07-30-prolog-graph-buy-verdict.md | M |
| V-059 | Time planes (event-stamp axis vs storage-retention axis) NOT unified; only the retention-emits-a-visible-minus finding and the R7 doc correction are executed | semantic-refusal | agent-verdict | fixture (receipts.sh 7 PASS 0 FAIL; recommendation items 2/3 left as design record only) | plans/2026-07-30-time-plane-unification-verdict.md | M |
| V-060 | Naive-referee's diff_local_line del:[] suppression for keep-clause rels REMOVED rather than inverted (a third suppression site the lab's own sweep had missed) | ruling | agent-verdict | measurement (naive sweep on both retention fixtures found wrong without the removal) | plans/2026-07-30-time-plane-unification-verdict.md | S |
| V-061 | Louvain/Leiden community detection: buy-the-offline-referee decision, no dl6 construct proposed (inner loop is a sequential fold reading in-pass mutations, not a rule firing over rows) | library-rejection | agent-verdict | measurement (section 3a reasoning, cites the same-pass dependency explicitly) | plans/2026-07-31-auto-factorization-verdict.md | L |
| V-062 | SCC condensation left as an engine gap (A4): mutual-reachability + min-index shape exists but costs a second full closure per graph, not run at scale | semantic-refusal | agent-verdict | measurement (section 7 shows one closure already walls under 1000 files) | plans/2026-07-31-auto-factorization-verdict.md | L |
| V-063 | min/max over a TEXT column REFUSED by both the compiler (aggregate_operand_not_number) and the reference engine (arithmetic error) | semantic-refusal | agent-verdict | fixture (R3d 4-line repro) | plans/2026-07-31-auto-factorization-verdict.md | S |

---

## Weak-trail decisions

Rows where evidence = none, or decided-by = unclear. Priority list for
re-evaluation (per the task: `none`/`unclear` are the valuable answers here,
not gaps in this inventory).

**160 of 245 rows.**

| id | decision | kind | decided-by | evidence |
|---|---|---|---|---|
| R-001 | `q1_occurrence_identity`: hybrid_stamps_plus_support_count: (tick,seq) stamps + engine-kept refCount as the one occurrence-identity semantics | ruling | user-ruled | none (review_occurrence_identity.md:117-135, doc pointer only) |
| R-002 | `q2_scoping`: occurrence scoping (Set vs Log) is an explicit rel-kind word on the decl | ruling | user-ruled | none (review_occurrence_identity.md:35-42) |
| R-003 | `q3_rel_kind_shape`: rel-kind is one word on the decl doing six jobs | ruling | user-ruled | none (AGGREGATE.md 1b) |
| R-004 | `q4_edge_propagation`: edge-written rows are arrivals for T+1, never same-tick, never dropped | ruling | user-ruled | none (review_temporal_pipe.md:120-124) |
| R-005 | `q5_drain_scheduler`: engine self-schedules drain ticks while the carry set is nonempty | ruling | user-ruled | none (code pointer only, temporal_pipe.pl:485-486) |
| R-006 | `q6_trigger_marker`: trigger marker = explicit per-atom marker (only/1); unmarked body = any-atom | ruling | user-ruled | none (review_temporal_pipe.md:15-23; later superseded for edge bodies by the C2 unmarked-trigger ruling) |
| R-007 | `q7_aggregate_multiplicity`: aggregate multiplicity = BAG of derivations (v5-SQL-compatible) | ruling | user-ruled | none (AGGREGATE.md Q7, reasoning only) |
| R-008 | `q8_key_vs_arrow`: Key() and `->` both live: Key = undirected uniqueness on state rels, `->` = program/world split on effect rels | ruling | user-ruled | none (AGGREGATE.md Q8 option (b); later superseded in spirit by decl_column_spelling killing Key() wrappers) |
| R-009 | `q9_aggregate_heads`: count/sum/min/max/json_array/json_object reserved as head-position aggregate forms | ruling | user-ruled | none (review_expressions.md:142-151) |
| R-010 | `q10_retention`: retention = required `keep <duration\|count>` clause on Log rels only | ruling | user-ruled | none (AGGREGATE.md Q10 option (a)) |
| R-011 | `r7_boundary_diff`: tick-boundary delta = multiset diff on Log rels, set diff on Set/level rels | ruling | user-ruled | none (code/test pointer only) |
| R-012 | `r_equal_row_write`: an equal-row keyed write is a no-op | ruling | user-ruled | none ('merge ambiguity 1', no data shown) |
| R-013 | `r1_rider_pre_chains`: `pre` chains across occurrences within a tick on fold rules | ruling | user-ruled | none (code pointer, occurrence_identity.pl) |
| R-014 | `json_arm`: json values are ordinary terms in the one value world; json_array/json_object build them | ruling | user-ruled | none (plans/2026-07-27-json-arm.md, directive only) |
| R-015 | `r4_departure`: departure is bindable via a `departed/1` body form, next tick, via carry | ruling | user-ruled | none (user question quoted, no measurement) |
| R-016 | `r6_pre_visibility`: `pre` reads the evolving store (T-1 when nothing wrote yet, chains after) | ruling | user-ruled | none ('the Q1 fold correctness depends on it', reasoning only) |
| R-018 | `cut_pipe`: `\|>` deferred from the construct budget (zero corpus chains at the time) | ruling | user-ruled | none (AGGREGATE 1d cut order row 1) |
| R-019 | `cut_quote`: `quote()` cut; evaluation-default stays a spec sentence | ruling | user-ruled | none (AGGREGATE 1d row 2) |
| R-020 | `s2_file_rels`: file rels split (mutable worktree vs immutable tree_file) unified by the File type | ruling | user-ruled | none (fs-rev-spine S2, doc pointer) |
| R-021 | `s3_dirtiness`: dirtiness is a derived rel, no Dirty(Oid) identity | ruling | user-ruled | none (fs-rev-spine S3, doc pointer) |
| R-022 | `storage_integer_keys`: integer surrogate keys everywhere in big graph storage; strings interned once | ruling | user-ruled | none (user quote, no measurement in this ruling (see strings-n1-verdict.md for the v5-side receipts)) |
| R-026 | `effect_abort`: effect abort = best-effort cancel on demand-support-zero, never a semantic guarantee | ruling | user-ruled | none (user invariant quote, no measurement of the lowering (owed, not yet built per the ruling text)) |
| R-028 | `spine_residency`: git/fs spine hosted in the language (stdlib rels+binds+salts), never kernel | ruling | user-ruled | none (user directive, no measurement) |
| R-029 | `clock_residency`: wall-clock cadence enters as a world-fed bind row, never a new construct | ruling | user-ruled | none (cites ghcacher F2 finding, not a measurement of this ruling itself) |
| R-030 | `lifecycle_arm_vocabulary`: lifecycle arm words = verbatim rx Observer vocabulary (next/finalize/unsubscribe/complete/subscribe/error); SQL trigger family rejected | ruling | user-ruled | none (user overruled the match-frontier lab's own SQL-trigger-family recommendation) |
| R-031 | `match_block_word`: the block word for arm dispatch is `match`, not partition/groupBy | ruling | user-ruled | none (user overruled the lab's own partition/groupBy pricing) |
| R-032 | `transition_rule_semantics`: boundary-collapsed transitions are first-to-last with mandatory collapse logging | ruling | user-ruled | none (user accepted the match-frontier lab's C2 crack as semantics, added a logging obligation) |
| R-033 | `rel_default_policy`: a bare rel is `value, unkeyed`; entity remains the marked case | ruling | user-ruled | none (overrides the round-2 types lab's 'no implicit policy' amendment) |
| R-034 | `enum_variant_separator`: enum variant separator is prolog's own semicolon | ruling | user-ruled | none (user quote, no measurement) |
| R-035 | `decl_column_spelling`: decl columns are `name(col: type, ...)`, colon-typed, source order significant; kills Key()/Min() wrappers | ruling | user-ruled | none (user quote, no measurement; wave-2 migration (53 kind(Ref,set) deletions, 49 files) is downstream execution, not evidence for the ruling itself) |
| R-036 | `enum_decl_in_rel`: enum variants live in the rel decl as prolog functors with the semicolon separator | ruling | user-ruled | none ('on the lowering argument', cites the types-lab enum-shape slot generally, no fresh measurement) |
| R-037 | `no_policy_suffix_words`: no policy suffix words on decls; `set` removed, `log` is the one kind word | ruling | user-ruled | none (user quote citing the types-lab verdict's own 'optional sugar' line) |
| R-038 | `edb_definition`: EDB is defined by absence: a never-headed rel is pure subject, no decl word marks it | ruling | user-ruled | none (user quote; reclassifies the binds-arc __lit_0 finding as a defect) |
| R-039 | `host_residency`: rows stay out of host (TS) residency; host sees deltas/aggregates, never a materialized table | ruling | user-ruled | none (user quote naming the scale-bench 10x gap and s3 OOM as the named suspects, not yet measured as caused by this) |
| R-040 | `expression_residency`: comparisons/arithmetic/string expressions fuse into emitted SQL; TS deopt only where sqlite lacks the function | ruling | user-ruled | none (user quote, reasoning only) |
| R-041 | `json_ticklog_encoding`: tick log renders json values as canonical JSON text, not prolog cons-term text | ruling | user-ruled | none (user chose from a multiple-choice round, no measurement in the ruling itself (regrade arc executed the consequence)) |
| R-043 | `keyed_level_head`: keyed() on a level-rule head is a compile error, not silent inert accumulation | ruling | user-ruled | none (user chose 'Compile error' from a multiple-choice round) |
| R-044 | `retention_count_lowering`: keep(count(N)) is lowered for real as a retracting rule over the log | ruling | user-ruled | none (user chose 'Lower it for real' from a multiple-choice round) |
| R-046 | `watcher_dep`: stay on node fs.watch behind IWatchSource; @parcel/watcher only on a measured bench regression | ruling | user-ruled | none (user quote, deliberately deferred until a bench regression is measured) |
| R-047 | `struct_arrival_key_order`: struct arrival key order is insignificant; oracle canonicalizes at load from the decl | ruling | user-ruled | none (user quote, no measurement) |
| R-048 | `bool_column_type`: bool becomes a real 2VL column type, overruling the earlier row-presence/enum golden-plan shape | ruling | user-ruled | none (user quote, ergonomics argument only) |
| R-049 | `numeric_precision`: float/REAL + avg() approved; precision spelling designed inside the phase-5 arc | ruling | user-ruled | none (user quote, approval only) |
| R-051 | `match_arm_tokens`: the `\|->`/`\|+>` match arm token pair is ratified, left-to-right reading order is the stated reason | ruling | user-ruled | none (user quote; 23 migrated fixtures are downstream execution, not evidence for the token choice itself) |
| R-052 | `json5_subset`: json5 subset = unquoted keys only, no trailing commas, no # comments | ruling | user-ruled | none (user quote) |
| R-053 | `list_spelling`: list type spelling is `list(type)` | ruling | user-ruled | none (user quote) |
| R-054 | `string_quote`: string literals parse under both quote styles | ruling | user-ruled | none (user quote) |
| R-055 | `descent_depth_cap`: `**` descent stays uncapped, like the CSS descendant combinator | ruling | user-ruled | none (user quote, reversibility argument (cap addable later, not removable)) |
| R-056 | `json_pattern_goal_spelling`: decode/2 named body atom chosen over a `body = {..}` operator on migration-cost grounds | ruling | coordinator | none (user delegated to 'whatever is easiest to change later'; coordinator's own reasoning, not measured) |
| R-057 | `scan_surface`: no new surface for scan-shaped programs; canonical spelling is keyed accumulator + log + match-block arms | ruling | user-ruled | none (user quote, deferred sugar until repetition shows the ugliness) |
| R-058 | `openapi_spec_artifact`: the generated OpenAPI spec is a checked-in artifact with a staleness gate | ruling | user-ruled | none (user quote) |
| R-059 | `openapi_route_list_generated`: the route list is generated from facts, not hand-kept twice | ruling | user-ruled | none (user quote) |
| R-060 | `openapi_generated_code_checked_in`: both spec and generated code are checked in | ruling | user-ruled | none (user quote) |
| R-062 | `stream_ordinal_spelling`: stream ordinal = seq(name) column-type sugar; engine-minted @ binding is dead | ruling | user-ruled | none (user quote ('i HATE the @ symbol')) |
| R-063 | `zip_reserved_row`: deleting the reserved `zip` row would make a typo a silent empty EDB; keep the refusal, name the equijoin in the message | ruling | user-ruled | none (user quote ('do the least fucky thing')) |
| R-064 | `stream_backpressure`: backpressure = watermark-gated writer, visible overflow rel, zero new constructs | ruling | user-ruled | none (user quote; CSP/clock follow-up banked as a future arc, not built) |
| R-065 | `latest_over_log`: latest() over a Log rel refuses at load, naming the max(Ordinal) rewrite | ruling | user-ruled | none (card 5b reasoning only) |
| R-066 | `stream_decl_word`: no dedicated 'stream' decl word; log+ordinal+keep already state the definition | ruling | user-ruled | none (card 6a reasoning only) |
| R-068 | `json_null_token`: json null = a reserved ground compound term, never a bare atom | ruling | user-ruled | none (user proposed (), reasoned into a compound to avoid atom-collision with text values) |
| R-069 | `json_dup_key_fate`: emitter refuses on json duplicate keys, matching the oracle's existing throw | ruling | user-ruled | none (user quote ('emitter throws if oracle throws')) |
| R-070 | `vocabulary_tiebreak`: naming ties break toward SQLite spelling first, then ANSI SQL; rx/prolog words only where no storage-plane spelling exists | ruling | user-ruled | none (user quote; does not itself trigger the B8 non-SQL word renames) |
| R-071 | `seq_sugar`: seq sugar approved, M2 (cursor numbering) only; M1 scan and M3 stages stay unwired | ruling | user-ruled | none (user: 'approve seq') |
| R-072 | `release_gate_v620`: v6.2.0 push/tag gated on ARCH-MAP.md generated from a single dl6 file (python renderer must go) | ruling | user-ruled | none (user quote, gate condition not evidence) |
| R-073 | `devlog_rail`: approve a dl6 program that reads session ledgers and emits DEVLOG.md | ruling | user-ruled | none (user quote ('docs YES DOGFOOD DOCS')) |
| R-075 | `bench_reference`: big-scale reference engine (tsv2 first) earns reference status by byte-proof vs the swipl oracle across the whole reachable corpus | ruling | user-ruled | none (user quote, sets a future measurement bar rather than citing one) |
| R-076 | `type_gate_widening`: decl-type arrival refusal gate widens to all column types/positions; coercion follows SQLite affinity | ruling | user-ruled | none (user quote ('do what sql would do')) |
| R-077 | `wide_int_fate`: integers beyond 2^53-1 refuse everywhere (named int_out_of_range) with a TODO marking a future bigint door | ruling | user-ruled | none (user quote, explicit deferral) |
| R-078 | `files_naming`: file enumeration hosts are `files(glob,...)` (unmarked worktree) vs `files_at(rev, glob,...)` (marked pinned rev); word `scan` banned | ruling | user-ruled | none (user quote, consistent with the standing spine_residency ruling) |
| R-079 | `org_fanout`: repo list is an ordinary sh host on a 1-day clock bind, fan-out via ordinary joins | ruling | user-ruled | none (user quote, zero new constructs claimed but not measured here) |
| R-080 | `gen_word_banned`: the word `gen` is banned for the codegen-sink construct; naming must come from rx/prolog/SQL vocabulary | ruling | user-ruled | none (user quote ('gen needs a new name i hate the name gen')) |
| R-081 | `repo_column_spelling`: repo-scoped enumeration is its own host pair (repo_files/repo_files_at), never a required leading cwd-literal column | ruling | user-ruled | none (user quote, follows the repo_grep_at precedent) |
| N-001 | Refuse an aggregate whose GROUP BY key is not delta-local (would force a full-table recompute) | named-refusal | agent-verdict | none |
| N-003 | Compiler refuses `aggregate_head_mixed_with_plain_clause` (aggregate head mixed with plain clause) | named-refusal | agent-verdict | none |
| N-004 | Compiler refuses `aggregate_head_no_positive_body` (aggregate head no positive body) | named-refusal | agent-verdict | none |
| N-005 | Compiler refuses `aggregate_head_reads_itself` (aggregate head reads itself) | named-refusal | agent-verdict | none |
| N-008 | Compiler refuses `aggregate_kind_not_lowered` (aggregate kind not lowered) | named-refusal | agent-verdict | none |
| N-009 | Compiler refuses `aggregate_operand_not_number` (aggregate operand not number) | named-refusal | agent-verdict | none |
| N-010 | Compiler refuses `aggregate_ordinal_not_int` (aggregate ordinal not int) | named-refusal | agent-verdict | none |
| N-011 | Compiler refuses `aggregate_separator_not_constant` (aggregate separator not constant) | named-refusal | agent-verdict | none |
| N-012 | Compiler refuses `arith_operand_not_int` (arith operand not int) | named-refusal | agent-verdict | none |
| N-013 | Compiler refuses `arith_operand_not_number` (arith operand not number) | named-refusal | agent-verdict | none |
| N-014 | Compiler refuses `at` (at) | named-refusal | agent-verdict | none |
| N-015 | Compiler refuses `bind_mismatch` (bind mismatch) | named-refusal | agent-verdict | none |
| N-016 | Compiler refuses `coalesce_in_head` (coalesce in head) | named-refusal | agent-verdict | none |
| N-017 | Compiler refuses `coalesce_multiple_outputs` (coalesce multiple outputs) | named-refusal | agent-verdict | none |
| N-018 | Compiler refuses `coalesce_no_output` (coalesce no output) | named-refusal | agent-verdict | none |
| N-019 | Compiler refuses `coalesce_not_top_level` (coalesce not top level) | named-refusal | agent-verdict | none |
| N-020 | Compiler refuses `coalesce_output_not_column` (coalesce output not column) | named-refusal | agent-verdict | none |
| N-021 | Compiler refuses `coalesce_source_not_rel_atom` (coalesce source not rel atom) | named-refusal | agent-verdict | none |
| N-022 | Compiler refuses `column_mismatch` (column mismatch) | named-refusal | agent-verdict | none |
| N-023 | Compiler refuses `column_ref_type_conflict` (column ref type conflict) | named-refusal | agent-verdict | none |
| N-024 | Compiler refuses `column_type_unknown` (column type unknown) | named-refusal | agent-verdict | none |
| N-026 | Compiler refuses `concat_non_display_piece` (concat non display piece) | named-refusal | agent-verdict | none |
| N-027 | Compiler refuses `concat_not_a_list` (concat not a list) | named-refusal | agent-verdict | none |
| N-028 | Compiler refuses `decode_field_unknown` (decode field unknown) | named-refusal | agent-verdict | none |
| N-029 | Compiler refuses `decode_pattern_not_object` (decode pattern not object) | named-refusal | agent-verdict | none |
| N-030 | Compiler refuses `decode_source_not_bound` (decode source not bound) | named-refusal | agent-verdict | none |
| N-031 | Compiler refuses `decode_source_not_struct` (decode source not struct) | named-refusal | agent-verdict | none |
| N-032 | Compiler refuses `edge_body_multiple_finalize` (edge body multiple finalize) | named-refusal | agent-verdict | none |
| N-039 | Refuse an edge-rule head write judged at risk of conflicting with another writer of the same row | named-refusal | agent-verdict | none |
| N-040 | Refuse an edge-rule write into a Set rel with no key declared | named-refusal | agent-verdict | none |
| N-041 | Refuse an edge trigger atom that is not a Log rel where Log is required | named-refusal | agent-verdict | none |
| N-042 | Compiler refuses `enum_variant_column_shape` (enum variant column shape) | named-refusal | agent-verdict | none |
| N-044 | Compiler refuses `enum_variant_shape` (enum variant shape) | named-refusal | agent-verdict | none |
| N-046 | Refuse a guard-goal shape the compiler cannot lower | named-refusal | agent-verdict | none |
| N-047 | Compiler refuses `head_arithmetic` (head arithmetic) | named-refusal | agent-verdict | none |
| N-048 | Compiler refuses `head_expr` (head expr) | named-refusal | agent-verdict | none |
| N-049 | Compiler refuses `host_executor_mismatch` (host executor mismatch) | named-refusal | agent-verdict | none |
| N-050 | Compiler refuses `int_out_of_range` (int out of range) | named-refusal | agent-verdict | none |
| N-052 | Compiler refuses `json_capture_type_unknown` (json capture type unknown) | named-refusal | agent-verdict | none |
| N-053 | Compiler refuses `json_key_contains_quote` (json key contains quote) | named-refusal | agent-verdict | none |
| N-054 | Compiler refuses `json_key_shape` (json key shape) | named-refusal | agent-verdict | none |
| N-055 | Compiler refuses `json_pattern_shape` (json pattern shape) | named-refusal | agent-verdict | none |
| N-056 | Compiler refuses `json_value_expression` (json value expression) | named-refusal | agent-verdict | none |
| N-060 | Compiler refuses `keyed_conflict` (keyed conflict) | named-refusal | agent-verdict | none |
| N-064 | Refuse a level-rule body goal shape the compiler does not recognize | named-refusal | agent-verdict | none |
| N-065 | Refuse a level rule with no positive body atom | named-refusal | agent-verdict | none |
| N-066 | Reserve rx lifecycle words (next/finalize/subscribe/unsubscribe/complete/error) from redefinition as ordinary rel/rule names | named-refusal | user-ruled | none (ruling lifecycle_arm_vocabulary) |
| N-067 | Compiler refuses `list_element_not_scalar` (list element not scalar) | named-refusal | agent-verdict | none |
| N-068 | Compiler refuses `list_of_relation_refs` (list of relation refs) | named-refusal | agent-verdict | none |
| N-070 | Compiler refuses `match_arm_head_not_positive_rel` (match arm head not positive rel) | named-refusal | agent-verdict | none |
| N-071 | Compiler refuses `match_arm_shape` (match arm shape) | named-refusal | agent-verdict | none |
| N-073 | Compiler refuses `match_source_not_positive_rel` (match source not positive rel) | named-refusal | agent-verdict | none |
| N-075 | Compiler refuses `negated_guard_goal` (negated guard goal) | named-refusal | agent-verdict | none |
| N-076 | Compiler refuses `non_finite_float_literal` (non finite float literal) | named-refusal | agent-verdict | none |
| N-077 | Refuse `now/1` tick-counter read inside a level-rule body | named-refusal | agent-verdict | none |
| N-078 | Compiler refuses `openapi_type_unknown` (openapi type unknown) | named-refusal | agent-verdict | none |
| N-081 | Compiler refuses `pattern_arg` (pattern arg) | named-refusal | agent-verdict | none |
| N-083 | Compiler refuses `probe_mismatch` (probe mismatch) | named-refusal | agent-verdict | none |
| N-084 | Compiler refuses `query_mismatch` (query mismatch) | named-refusal | agent-verdict | none |
| N-085 | Compiler refuses `quote_in_literal` (quote in literal) | named-refusal | agent-verdict | none |
| N-086 | Refuse a rule graph whose level-rule dependency forms a cycle (non-stratifiable negation/recursion) | named-refusal | agent-verdict | none (matches the standing not_stratified guard; tabling-verdict lab confirms this IS semantics, not an artifact) |
| N-087 | Compiler refuses `seq_cursor_name_collision` (seq cursor name collision) | named-refusal | agent-verdict | none |
| N-088 | Compiler refuses `seq_in_level_rule` (seq in level rule) | named-refusal | agent-verdict | none |
| N-089 | Compiler refuses `seq_partition_type_unknown` (seq partition type unknown) | named-refusal | agent-verdict | none |
| N-090 | Remove `set` as a decl suffix word; a bare rel is already a set table | named-refusal | user-ruled | none (ruling no_policy_suffix_words) |
| N-091 | Compiler refuses `sql_text_mismatch` (sql text mismatch) | named-refusal | agent-verdict | none |
| N-092 | Compiler refuses `surface_findings` (surface findings) | named-refusal | agent-verdict | none |
| N-094 | Compiler refuses `template_mismatch` (template mismatch) | named-refusal | agent-verdict | none |
| N-095 | Compiler refuses `text_operand_not_text` (text operand not text) | named-refusal | agent-verdict | none |
| N-096 | Compiler refuses `trigger_arg_not_var` (trigger arg not var) | named-refusal | agent-verdict | none |
| N-097 | Compiler refuses `type_arrival_shape_mismatch` (type arrival shape mismatch) | named-refusal | agent-verdict | none |
| N-098 | Compiler refuses `unbound_head_var` (unbound head var) | named-refusal | agent-verdict | none |
| N-099 | Compiler refuses `unknown_comparison_operator` (unknown comparison operator) | named-refusal | agent-verdict | none |
| N-100 | Compiler refuses `value_template_never_shipped` (value template never shipped) | named-refusal | agent-verdict | none |
| N-101 | Reserve `zip` as a construct name; refuse redefining it as an ordinary rel/rule name | named-refusal | user-ruled | none (ruling zip_reserved_row) |
| V-003 | Dataflow node identity candidate (A) dense sequence with no dictionary REJECTED (crash-resumed slice would mint different ids) | ruling | agent-verdict | none (reasoning only, 'ruled out exactly as the mandate anticipated') |
| V-006 | AsyncLocalStorage rejected for trace context propagation; explicit tick/rel args used instead | library-rejection | agent-verdict | none (cites external ALS overhead benchmarks (4-12%), not measured in this repo) |
| V-007 | node:diagnostics_channel tracingChannel + pino CHOSEN as the tracing spine/emit layer over OpenTelemetry JS and a hand-rolled fs.appendFile writer | library-rejection | user-ruled | none (user approved the one new dep (pino) per the ledger; doc itself just proposes) |
| V-008 | OpenTelemetry JS deferred as an escalation path, not rejected outright (becomes right only when a cross-runner schema is needed) | library-rejection | agent-verdict | none (reasoning only) |
| V-012 | Pacing (b), one-per-drain-tick, PROPOSED as the queue-pacing spelling (SLOT-QUEUE-PACING); not yet user-ratified at doc time | semantic-refusal | unclear | measurement (lab proposal only, marked 'PROPOSED (b), with the drain-cap cost named' in the slot table) |
| V-020 | SQL trigger family (inserted/deleted/OLD/NEW) priced as the update-arm spelling, then user-overruled in favor of rx Observer words | semantic-refusal | user-ruled | none (superseded by ruling lifecycle_arm_vocabulary) |
| V-026 | Struct/enum spelling (b) prolog functors CHOSEN over (c) plain rels and (a) json braces | ruling | agent-verdict | none ('criteria visible, no fiat', worked example only, no measurement cited) |
| V-030 | Zero-decl rel-name bind activation REFUSED as a magic-rel hazard; `bind interval(...)` selected instead | semantic-refusal | agent-verdict | none (reasoning only, cites the v5 magic-rel ban) |
| V-035 | Derived rels REFUSED as spread sources (spread_source_not_declared); only generated (enum-expanded) or plain decl sources allowed | semantic-refusal | agent-verdict | none (reasoning: 'the real line is GENERATED vs INFERRED') |
| V-040 | finalize over a Log rel is silently dead with no refusal (U5); load-time refusal RECOMMENDED, not yet built (SLOT-LOG-FINALIZE-REFUSAL) | semantic-refusal | unclear | measurement (recommendation only, decidable per the doc, not executed at verdict time) |
| V-044 | buf CLI JSON descriptor output priced but NOT verified/adopted (not installed, documentation-only row) | library-rejection | agent-verdict | none (explicitly marked 'not verified' in the doc's own table) |
| V-047 | JSON_checker (Crockford) REJECTED: strict subset of JSONTestSuite, no license file | library-rejection | agent-verdict | none (comparison against JSONTestSuite coverage) |
| V-048 | google/json-test-suite REJECTED: no per-case accept/reject classification | library-rejection | agent-verdict | none (reasoning only) |
| V-049 | fast-check/jsverify REJECTED as a dependency, its round-trip SHAPE adopted as a fixed 136-value enumeration instead of seeded random generation | library-rejection | agent-verdict | none (reasoning: cross-target contract receipts must be reproducible/diffable) |
| V-050 | Bespoke json test-document generator REFUSED (JSONTestSuite already fits and is what other implementations report against) | library-rejection | agent-verdict | none (reasoning only) |

---

## Counts

Total rows: **245** (rulings 81, named refusals 101, verdict-doc decisions 63).

By kind:

- named-refusal: 101
- ruling: 99
- semantic-refusal: 28
- library-rejection: 17

By decided-by:

- agent-verdict: 153
- user-ruled: 89
- unclear: 2
- coordinator: 1

By evidence class:

- none: 158
- measurement: 53
- fixture: 34

Weak-trail total: **160** (65% of all rows).

---

## Sources not fully re-verified line-by-line

The 23 verdict docs range from 106 to 1,324 lines; each was read for its
VERDICT/headline/slot sections and per-item decision tables, not every line.
Docs with large `OPEN`/`PROPOSED` slot inventories (consumption-arms,
match-frontier, csp-idioms, type-matrix) have unresolved questions beyond
what is rowed here; those slots are not decisions yet and are intentionally
omitted rather than force-fit into a row. `plans/2026-07-30-type-matrix-
verdict.md`'s 422-cell empirical matrix produced no new named decision beyond
feeding the already-rowed `type_gate_widening` ruling, so it has no dedicated
V- row.

