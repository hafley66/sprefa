% rulings.pl : the ruling queue (AGGREGATE.md section 3) as facts the engine
% reads. Status: RULED BY THE USER 2026-07-27 (asked one by one;
% every advisory confirmed). A later override is one fact flip + a fixture
% re-grade.
%
% ruling(Id, Choice, provisional | user, Receipt).

:- module(rulings, [ruling/4]).

% Q1: within-tick occurrence identity.
% hybrid = A's semantics (engine stamps (tick, seq) on event rels; order and
% multiplicity survive; folds chain per occurrence) plus the store retaining
% its IVM support count as engine bookkeeping. There is no third semantics.
ruling(q1_occurrence_identity, hybrid_stamps_plus_support_count, user,
       'review_occurrence_identity.md:117-135').

% Q2/Q3: occurrence scoping is an explicit rel-kind word on the declaration.
% Set = membership (identical content is the same thing, dedup holds).
% Log  = occurrence (every arrival is a new thing, stamps carried, append-only).
% Bind-filled rels infer from Stream/Tail result wrappers.
ruling(q2_scoping, explicit_rel_kind_declaration, user,
       'review_occurrence_identity.md:35-42').
ruling(q3_rel_kind_shape, kind_word_on_rel_decl, user,
       'AGGREGATE.md 1b: one word, six jobs').

% Q4: edge-written rows are arrivals for T+1, never same-tick, never dropped.
ruling(q4_edge_propagation, next_tick, user,
       'review_temporal_pipe.md:120-124').

% Q5: the ENGINE self-schedules drain ticks (empty outside-arrival set) while
% the carry set is nonempty; chains never freeze when outside arrivals stop.
ruling(q5_drain_scheduler, engine_owned, user,
       'temporal_pipe.pl:485-486 smuggled this in; now owned').

% Q6: trigger marker = explicit per-atom marker (spelled only/1 in the
% reference engine; surface spelling still surface_dcg's call). A body with
% markers fires on marked atoms only; an unmarked body keeps any-atom.
ruling(q6_trigger_marker, explicit_marker, user,
       'review_temporal_pipe.md:15-23').

% Q7: aggregate multiplicity = BAG of derivations (v5-SQL-compatible; two
% hits on one line count 2). The rail lab's set behavior is the rejected
% reading, kept in fixture comments.
ruling(q7_aggregate_multiplicity, bag, user,
       'AGGREGATE.md Q7: v5-compatibility favors bag').

% Q8: Key vs -> : both live, law stated: Key = undirected uniqueness on state
% rels; -> = the program/world column split on effect rels; det effects are
% where they coincide. (Least-commitment reading; the labs split three ways.)
ruling(q8_key_vs_arrow, both_with_stated_law, user,
       'AGGREGATE.md Q8 option (b)').

% Q9: count/sum/min/max (+ json_array/json_object, json arm 2026-07-27) are
% reserved head-position aggregate forms, excluded from stdlib + expr grammar.
ruling(q9_aggregate_heads, reserved_head_forms, user,
       'review_expressions.md:142-151').

% Q10: retention = per-rel `keep <duration|count>` clause, REQUIRED on Log
% rels; ranges over Log rels only; under q1 hybrid it is a tick-prefix DELETE.
ruling(q10_retention, keep_clause_required_on_log, user,
       'AGGREGATE.md Q10 option (a)').

% Residuals the engine also implements:
% R7: the tick-boundary delta set is a delta MULTISET on Log rels (one delta
%     per new stamp), a set diff on Set/level rels.
ruling(r7_boundary_diff, multiset_on_log_set_on_set, user,
       'occurrence store_deltas; check_eventing #1').
% equal-row keyed write = no-op (written_at column serves SWR later).
ruling(r_equal_row_write, noop, user, 'merge ambiguity 1').
% R1 rider: pre chains across occurrences WITHIN a tick on fold rules (the
% occurrence lab's semantics; what makes the fold correct).
ruling(r1_rider_pre_chains, chains_within_tick, user,
       'occurrence_identity.pl apply_occurrence').

% JSON arm (user directive 2026-07-27): json values are ordinary terms in the
% one value world (obj/list/str/int/bool/none share the struct braces), and
% aggregate heads json_array/json_object build them. See
% plans/2026-07-27-json-arm.md.
ruling(json_arm, terms_plus_aggregate_heads, user,
       'user 2026-07-27: json part of the type language, braces and all').

% Rulings taken in the 1-by-1 session, 2026-07-27 PM:

% R4: departure IS bindable. Retraction was already an event (R7 emits -Row
% for Set/level rels); one body form (departed/1 in the reference engine,
% surface spelling surface_dcg's call) fires on it NEXT tick via the carry,
% stamped, marker-scopable. Only Set/level rels can depart.
ruling(r4_departure, departure_body_form_adopted, user,
       'user: "is retraction not a built-in event we can match on?"').

% R6: pre reads the EVOLVING store (T-1 exactly when nothing wrote yet;
% later occurrences chain). Frozen-snapshot pre is the rejected reading.
ruling(r6_pre_visibility, evolving_read, user,
       'the Q1 fold correctness depends on it').

% A6: diag is an ORDINARY rel declared by std/diag; the CLI is a consumer.
% The engine never knows the name (the v5 magic-rel ban holds in v6).
ruling(a6_diag, ordinary_rel, user, 'timeless_rail fixtures model it so').

% Construct-budget cuts: |> deferred (zero corpus chains; fixtures test the
% desugared rules and stay valid); quote() cut (evaluation-default RULE
% stays as a spec sentence). Inventory: 30 - 2 cuts + 1 departure form = 29.
ruling(cut_pipe, deferred, user, 'AGGREGATE 1d cut order row 1').
ruling(cut_quote, cut_keep_eval_default_rule, user, 'AGGREGATE 1d row 2').

% Spine rulings (plans/2026-07-27-fs-rev-spine.md):
% S2: SPLIT file rels (mutable worktree keyed by path; immutable tree_file
% keyed by (rev, path), lazy) unified by the File type.
ruling(s2_file_rels, split_unified_by_file_type, user, 'fs-rev-spine S2').
% S3: dirtiness is a DERIVED rel (worktree digest vs tree_file at head);
% no Dirty(Oid) rev identity, no alias machinery.
ruling(s3_dirtiness, derived_rel, user, 'fs-rev-spine S3').

% Storage law: integer surrogate keys EVERY time in the big graph storage;
% strings/hashes live once in interning tables, read at the presentation
% edge. No string FKs.
ruling(storage_integer_keys, dense_int_surrogates, user,
       'user 2026-07-27: "pick integers every time u can"').

% N+1 law, lowering tier: statements per tick = f(rules, strata), never
% f(rows). The reference engine is per-row ON PURPOSE (it is the spec);
% every lowering carries the flat-statement budget, graded by the
% statement-budget rail (fixture at 1x vs 100x data, identical counts).
ruling(n1_statement_budget, flat_per_tick_statement_count, user,
       'v5 tick-counter law promoted into a graded conformance check').

% Stale fill under a dead scope (sub-forest ambiguity 1). SUPERSEDED same
% day: the first take ("surface orphan fills as a rel, program decides")
% added a delivery-guarantee dimension the user rejected ("we're not
% AMQP"). Final ruling: CANCELLATION IS THE KERNEL PRIMITIVE. Demand-row
% deletion IS the abort signal (Go-context shape): when an effect's
% demand support hits zero, the in-flight run is aborted (process killed,
% fetch aborted, timer cancelled) and its pending cache row deleted.
% There is no orphan fill by construction; the only residual is the
% same-tick race (teardown and fill in one tick), which the absorption
% arc grades both ways and picks one. Wanting a fill to outlive your
% scope is not a fill policy, it is DEMAND FROM A LONGER-LIVED SCOPE
% (the cache rule demands the fetch; the UI joins the cache rel).
% RESOLVED 2026-07-27 late PM by the salt ruling below: under
% content-addressed salts a fill is never stale, it is a cache update
% addressed to (identity, witness), valid for every scope that ever
% demands that identity (redteam-stale-fill B4: all three fill readings
% converge to the same store under content salts). No fill policy,
% no orphan rel, no per-instance identity, no fill tick-item.
ruling(stale_fill_policy, not_applicable_under_content_salts, user,
       'user 2026-07-27 late PM grunt 1; plans/2026-07-27-redteam-stale-fill.md decision shape Q1').

% Salt minting: CONTENT-ADDRESSED, always. The salt is witness data
% (content hash, clock bucket), never a subscription id. Two scopes
% demanding the same (identity, witness) share ONE in-flight effect and
% one cache row; teardown safety comes free from IVM support refcounting
% (sub-forest finding). Per-instance salts measured 12x world calls on
% the gh-cache retick probe. Freshness-on-purpose is spelled as an
% explicit extra salt column (nonce/bucket), data not policy.
ruling(salt_minting, content_addressed, user,
       'user 2026-07-27 late PM: "one hunt"; matches shipped TS effect_cache digests + v5 two-salts law').

% Abort on demand-support-zero: YES, as BEST-EFFORT world-cost machinery,
% never a semantic guarantee. User-stated invariant: "no one stop arrow,
% no arrow stop exist, is lie" -- a cancelled effect MAY still have
% spent/landed; correctness never depends on cancellation having worked
% (store semantics already do not: fills are cache updates per the salt
% ruling). The invariant carries a painted warning at the abort site and
% a debug/trace line on every abort attempt+outcome. Lowering owed:
% AbortSignal through HostDef.run + cancel-handle map + pending cache-row
% delete on abort (none exist today, 1_hosts.ts:387 filters inserts only).
ruling(effect_abort, best_effort_cancel_on_support_zero, user,
       'user 2026-07-27 late PM grunt 2: "rope arrow" + warn-paint invariant').

% Subscription kernel: MINIMAL. Zero stored semantic rels, zero new tick
% phases. switchMap = keyed replace on an ordinary program rel; flattening
% policy = the scope row primary key shape ([1]=switch, [1,2]=merge,
% [1]+guard=exhaust); concat queue, scope_done, demanded/2 are program
% rules; teardown = ordinary IVM retraction (counting kill measured FLAT,
% 21 statements at cone depth 1..256). Two OBLIGATIONS ride the ruling:
%   1. static scope-coverage check -- every rel derived under a scope
%      carries the scope key, refused otherwise (answers redteam A2b's
%      zombie-scope break; decidable, mode-lattice column-flow machinery);
%   2. ghost forest -- the scope tree derivable as a diagnostic view on
%      demand, never stored semantics (answers redteam A2 forensics).
% Filed separately, NOT part of this ruling: recursive rels inside scope
% cones force DRed at f(depth) statements in EVERY kernel design
% (redteam A1); that is a lowering/scheduler hazard against
% n1_statement_budget, owner unassigned.
ruling(subscription_kernel, minimal_with_coverage_check_and_ghost_view, user,
       'user 2026-07-27 late PM grunt 3; plans/2026-07-27-redteam-minimal-kernel.md verdicts').

% Spine residency: the git/fs spine (GitRepo, GitRev, File enumeration,
% watching, auto-synced repo lists) is HOSTED IN THE LANGUAGE -- stdlib
% rels + binds + salts over the generic effect machinery -- never kernel.
% They are not native language concepts; the native concepts (rels, keys,
% salts, demand, arrows) must accommodate their interactions intuitively,
% and every place they fail to is a language finding, not a reason to
% special-case the spine. Value types (File, GitRev as interned ints)
% stay types; the RELS over them are library.
ruling(spine_residency, stdlib_rels_and_binds_not_kernel, user,
       'user 2026-07-27 late PM: spine hostable in the language instead of being core').

% Clock residency: wall-clock cadence enters the system as a WORLD-FED BIND
% (rows arriving like any other input, e.g. clock_bucket(period, bucket)),
% never a new language construct and never kernel machinery. Rules cannot
% observe time passing (nothing re-fires without an input row changing), so
% time ticks in as ordinary arrivals; SWR/staleness policy is then plain
% rules over latest state (keyed replace = latest wins) joined against the
% clock rel -- the ghcacher F2 gap dissolves with zero construct-budget
% cost. Consistent with spine_residency (clock is spine-shaped: a bind) and
% with v5's clock(secs, bucket) builtin, re-homed as a bind per the ruling
% above. Lifetimes stay matches; no bespoke freshness machinery.
ruling(clock_residency, world_fed_bind_not_construct, user,
       'user 2026-07-28 AM: "clock bind yes"; ghcacher findings F2 + the SWR-as-latest-state exchange').

% 2026-07-28 PM, after the match-frontier lab verdict. The lab priced the
% SQL trigger family (inserted/deleted/OLD/NEW) first; the user overruled:
% the arm vocabulary is the rx Observer lifecycle, verbatim. "lifecycle
% must be next/finalize/unsubscribe/complete/subscribe/error etc. they are
% match arms in theory or built ins." Error-arm semantics stay subordinate
% to the failure-is-a-value envelope ruling (an error arm must not become
% a second failure channel); that reconciliation is design work, not ruled.
ruling(lifecycle_arm_vocabulary, rx_observer_words, user,
       'user 2026-07-28 PM: next/finalize/unsubscribe/complete/subscribe/error are the arm names; SQL trigger family rejected').

% Same sitting. Lab priced partition/groupBy above match for the block
% word; user overruled: the block word is match.
ruling(match_block_word, match, user,
       'user 2026-07-28 PM: "match" over partition/groupBy').

% 2026-07-28 evening. The match-frontier lab's C2 crack: a key replaced
% N times inside one boundary shows only first-to-last; the intermediate
% values are invisible and the transition count depends on batching. User
% accepts the collapse as the semantics AND requires it be observable:
% when a transition-consuming rule's boundary collapses multiple replaces
% (at any sync/next/async frontier: Tn, Ti drain, world batch), the
% runtime logs the collapse through the tracing spine (a debug event
% naming rel, key, and the number of collapsed occurrences), never
% silently. Same warn-paint discipline as effect_abort. Obligation
% attaches to the arms implementation arc.
ruling(transition_rule_semantics, first_to_last_per_boundary_with_collapse_logging, user,
       'user 2026-07-28 evening: "that is fine i think, we should log when that transition rule of sync/next/async boundaries happen"').

% 2026-07-28 night. Overrides the round-2 lab amendment "no implicit
% policy" for the bare case: a bare rel IS a table, and a table is
% already a replay subject (late readers get every current row), so the
% default policy is value, unkeyed. entity remains the marked case; the
% dual-policy form still requires explicit producer words. Enum decl
% spelling (the semicolon) explicitly NOT ruled; user not sold.
ruling(rel_default_policy, value_unkeyed, user,
       'user 2026-07-28 night: "rel default is value unkeyed, its a create table by itself, a literal dynamic replay subject"').

% 2026-07-28 night. The variant separator in enum-shaped decls is
% prolog's own disjunction: rel body(page(view: view) ; redirect(to:
% text)). Zero new tokens; the pun is the point.
ruling(enum_variant_separator, prolog_semicolon, user,
       'user 2026-07-28 night: "semicolon as OR is ... yes please"').

% 2026-07-29 (post-arms-lab sitting). Decl columns are the : operator,
% nothing else: rel name(col: type, ...), source order significant (the
% js-object-literal precedent, stated by the user against rust's
% unordered fields). Types are inherently a second key-value dimension;
% how they are STORED is the compiler's business. Kills the reserved
% Key(text)/Min(int) column wrappers as a type spelling. Head/body rule
% shapes are the LOWERING TARGET for matching and piping, not part of
% the decl surface question.
ruling(decl_column_spelling, colon_typed_ordered_columns, user,
       'user 2026-07-29: "there is quite literally no difference to the : operator ... js inline objects have their source order bc it just makes sense ... rel name(key: types)"').

% Same sitting. Enum variants live IN the rel decl as prolog functors
% with the already-ruled semicolon separator:
%   rel body(page(view: view) ; redirect(to: text)).
% Supersedes the "user not sold" note on rel_default_policy: user is
% now sold, on the lowering argument (N variant rels + derived tag
% view, per the types-lab enum-shape slot).
ruling(enum_decl_in_rel, semicolon_variants_in_decl, user,
       'user 2026-07-29: "get that semicolon freak in the lang please it has good lowering to sql its fine"').

% Same sitting. NO policy suffix words on decls. The word `set` is
% REMOVED from the surface: a bare rel is already a set table in the
% shipped engine (engine.pl rel_kind/4 fallback clause = set, and
% keyed(...) already implies set), so the word states the default and
% nothing else. The value/entity plane words from the coexistence lab
% never land as decl suffixes; the plane is carried by key(...) choice
% plus id binds, which the types-lab verdict itself proved sufficient
% ("the value policy word is optional sugar ... fully expressible with
% key(...) plus the id bind", verdict line 260). `log` remains the one
% explicit kind word (it changes semantics: append + retention), with
% keep(...)/key(...) as modifiers. Entity-plane EXTRAS (immutable
% history, explicit checked retirement) still need a future spelling;
% whatever it is, it will not be a bare suffix word on the decl.
ruling(no_policy_suffix_words, bare_rel_is_set_log_is_the_only_kind_word, user,
       'user 2026-07-29: "i dont want magic suffix words to trip anything up. no set. ... log all and a rel without any form of specificity are just tables no?"').

% 2026-07-29 same sitting. EDB is DEFINED BY ABSENCE: a rel (enum-shaped
% or not) that no rule ever heads is pure input -- an rx Subject the
% world pushes into (schedule rows, binds, host responses). No decl word
% marks it; being un-headed IS the mark. Consequence: the binds-arc
% finding that a bare fact like clock_period(2). compiles to an IDB rule
% over a minted __lit_0 seed is now a DEFECT, not a curiosity -- facts
% on a never-headed rel must seed EDB rows.
ruling(edb_definition, never_headed_rel_is_pure_subject, user,
       'user 2026-07-29: "we want edb to be ... a rel enum, that never has a body, is pure subject"').

% 2026-07-29 same sitting. The founding purpose of the TS runtime,
% stated after the scale bench landed: ROWS STAY OUT OF HOST RESIDENCY.
% Tables live in sqlite; the host process sees deltas and aggregates,
% never a materialized table. The naive boundary diff (full-table reads
% into JS for multisetDiff every tick) violates this and is the named
% suspect for both the 10x-vs-v1 overhead and the s3 OOM-at-1k. Grade
% consequence: zero full-table reads into JS anywhere in the tick path
% is an acceptance criterion for the emitter arc, not an optimization.
ruling(host_residency, rows_stay_in_sqlite_host_sees_deltas, user,
       'user 2026-07-29: "entire purpose of that ts engine was keeping rows out of residency in the host"').

% 2026-07-29. Expression lowering law: comparisons, arithmetic, and
% string expressions FUSE into the emitted SQL delta statements
% (sqlite's expression engine is the target; it has the coverage).
% Deopt to TypeScript ONLY where sqlite genuinely lacks the function,
% never as a default path -- "we fuse it to sql deltas in rx". The
% phase-C refusals (comparison/bind/head-arith) were guards against
% miscompiles whose causes typed columns removed; lifting them into
% the incremental emitter is now an arc, not a hazard.
ruling(expression_residency, fuse_to_sql_deltas_ts_deopt_last, user,
       'user 2026-07-29: "we can deopt into typescript but only if we have to otherwise we fuse it to sql deltas in rx"').

% 2026-07-29 (multiple-choice round, user tired but explicit). The
% tick log renders json values as CANONICAL JSON TEXT, not prolog
% cons-term text. Consequence: json_array/json_object aggregate
% heads become emittable (json_group_array/json_group_object +
% ORDER BY reproducing msort/keysort); the oracle's tick-log encoder
% changes once and affected fixtures regrade once. Supersedes the
% [|](...) rendering for json-typed values only; plain compounds
% keep canonical term text.
ruling(json_ticklog_encoding, canonical_json_text, user,
       'user 2026-07-29: chose "Canonical JSON in tick log" from the multiple-choice round').

% 2026-07-29. UDF residency for tsv2: STAY on @libsql (which has no
% registration API, proven empirically in the udf lab); coverage =
% core-SQL fusion where semantics match, TS deopt over DELTA ROWS
% ONLY otherwise (regex via the JS-compatible subset), emit-time for
% constants. Driver swap / rust sidecar deferred to the rust return.
ruling(udf_residency, libsql_fuse_and_delta_deopt, user,
       'user 2026-07-29: "there is truly no other way in ts" -- accepted stay-libsql from the multiple-choice round').

% 2026-07-29 (second multiple-choice round, after plain-words
% explanation). keyed() on a level-rule head is a COMPILE ERROR
% (named refusal) in oracle and tsv2 both; keyed replace stays edge
% semantics. Closes the silently-inert defect from the hands-on
% findings.
ruling(keyed_level_head, named_refusal, user,
       'user 2026-07-29: chose "Compile error" -- keyed stays an edge-rule thing').

% 2026-07-29. keep(count(N)) retention is LOWERED FOR REAL in tsv2:
% emitted as an ordinary retracting rule over the log rows beyond N,
% riding the landed P3 retraction SQL. The consumption lab's s1
% smallest-honest spelling. Closes the final_wrong retention rows.
ruling(retention_count_lowering, retracting_rule_over_log, user,
       'user 2026-07-29: chose "Lower it for real" from the second multiple-choice round').

% 2026-07-29 morning. Compound/struct column storage: STRUCT-AS-ROWS
% (the types-as-rels lab design executed). A declared struct value is
% a rel row referenced by content id; parent columns store the ref,
% never an inline blob. decode/2 dissolves into joins. The inline
% json-vs-term-text double spelling (the decode_arc blocker) ends.
% json1 remains for UNTYPED json only (the lab's SLOT-JSON1-FATE
% fill); typed fields are always refs. Log contract prerequisite
% carried from the lab as a hard requirement: the tick log prints
% rendered canonical value text, NEVER ids
% (rendered_text_stable_under_both_policies is the receipt shape).
ruling(compound_storage, struct_as_rows, user,
       'user 2026-07-29: "lol d" -- struct-as-rel over json-blob patch; tick-log + dictionary edges worked in plans/2026-07-29-struct-as-rows-header.md').

% 2026-07-29 morning. Watcher dependency (morning-list #3): STAY on
% node fs.watch behind the IWatchSource seam. The watcher lives and
% dies by lang/runtime -- the TS host binding is temporary (rust one
% day), so no further investment in the binding choice unless it
% borks a bench. @parcel/watcher remains the researched one-adapter
% swap, taken only on a measured bench regression, never proactively.
ruling(watcher_dep, fs_watch_until_bench_regression, user,
       'user 2026-07-29: "will live and die by lang/runtime ... dont put too much thought into it unless it borks any form of bench"').

% 2026-07-29 midday. Struct arrival key order (struct-as-rows arc slot
% SLOT-ARRIVAL-CANONICAL-ORDER): INSIGNIFICANT. The type declaration
% names the field set, so the canonical spelling is induced from the
% decl -- the oracle rewrites every world row to sorted-key obj/1 form
% at load (canonicalize_world_rows/3, run_program) instead of refusing
% out-of-order keys. keys_not_sorted is dead; missing/unknown/
% wrong-type refusals stay. The emitted runtime already canonicalized
% at intern; divergence stays unreachable, now from the accepting side.
ruling(struct_arrival_key_order, decl_induced_canonicalize, user,
       'user 2026-07-29: "we know the types order so we can induce it"').

% 2026-07-29 evening. Bool columns: the recorded golden-plan shape
% (bool = row presence / two-variant enum, never a column type) is
% OVERRULED as un-ergonomic. bool becomes a real column type, strictly
% two-valued (2VL: true/false literals, no null, no unknown -- absence
% stays row-absence). Storage/lowering spelling (INTEGER 0/1, literal
% words vs atoms, guard interplay) rides the phase-5 type-pass arc.
ruling(bool_column_type, two_valued_column_type, user,
       'user 2026-07-29: "how do we not need bools come on now, even if they are just 2vl and not 3vl ... that is un ergonomic"').

% 2026-07-29 evening. Numeric precision: approved. float/REAL + avg()
% (the phase-5 hole) gets its yes; precision spelling (REAL vs
% fixed-decimal for exactness-sensitive columns) is designed inside
% the same arc, not assumed.
ruling(numeric_precision, approved_phase5_design, user,
       'user 2026-07-29: "and yes to precision numbers"').

% 2026-07-30. The json key hole marker. CARD-KEY-HOLE-SPELLING is
% ruled DOLLAR: a key-position hole is written `$name`, matching the
% value-position hole, so `{ $key: $value }` reads uniformly on both
% planes. This unblocks 4 of the 5 constructs the recovery doc graded
% "needs new surface"; the lowering was already proven to be
% json_each(key,value) with zero new SQL (json_syntax lab L3).
ruling(json_key_hole_marker, dollar, user,
       'user 2026-07-30: "a key hole is $ for the lulz"').

% 2026-07-30. The match arm token pair minted by 9cadb419 (|-> and
% |+>) is RATIFIED, and the authorship question is settled: the user
% asked for them. Reason of record: left-to-right reading order --
% guards first, then the arrow, so an arm reads in the direction the
% data flows. Standing intent attached: rel programs should look
% uniform and be able to express flow ACROSS TIME. The finding
% match_arm_new_tokens_unruled is closed by this row; the 23 migrated
% .dl6 fixtures stand.
ruling(match_arm_tokens, ratified_ltr_pair, user,
       'user 2026-07-30: "i was the one that wanted |-> so we had ltr reading nature, i want these programs with rels looking sexy and uniform and able to express flow across times"').

% 2026-07-30 json card round. User rulings, recorded verbatim in the
% justification field.
ruling(json5_subset, unquoted_keys_only, user,
       'user 2026-07-30: "just unquoted keys". Trailing commas and # comments are NOT taken; the subset is exactly json plus bare identifier keys').
ruling(list_spelling, list_of_type, user,
       'user 2026-07-30: "list spelling is list(text) seems easy enough"').
ruling(string_quote, both_parse, user,
       'user 2026-07-30: "sring quote: both"').
ruling(descent_depth_cap, uncapped, user,
       'user 2026-07-30: "descent depth nah, css aint got it". ** stays unbounded like the CSS descendant combinator; a cap can be added later without breaking programs, a cap removed later cannot').
% The pattern-goal spelling was delegated to the cheapest-to-migrate option.
% decode(body, {..}) is a named body atom: a functor rename is a mechanical
% sweep across parser, printer, registry and fixtures. `body = {..}` bakes the
% meaning into an OPERATOR, which is parsed, printed, registry-listed and
% grammar-highlighted as syntax, and = already carries an unrelated meaning
% elsewhere. So the named goal is the reversible choice.
ruling(json_pattern_goal_spelling, named_goal_decode, user,
       'user 2026-07-30: "do whatever is easiest to change later for decode/pattern-goal spelling". Coordinator picked the named body atom over the operator form on migration cost').

% 2026-07-30. Scan surface. The rel-as-stream lab returned an EMPTY tier-0
% list, so nothing about scan justifies new syntax. Ruling: NO new surface for
% now. The canonical spelling is the one the lab already grades: a keyed state
% rel for the accumulator, a log rel for the sequence, and a match block whose
% arms are ordinary |+> edge rules. It is the closest thing to tier 0 that
% exists, because it is made entirely of constructs that already ship. Write
% real programs with it, let the ugliness show up under repetition, and sugar
% it afterwards from evidence instead of from a guess.
ruling(scan_surface, no_new_surface_match_block_arms, user,
       'user 2026-07-30: "take whatever is the simplest and closest to tier0 for now and explain it to me, we use it alot and then see how ugly consistent it is and give it suytnax later"').

% 2026-07-30. openapi lab card 1. The generated spec is a CHECKED-IN artifact
% with a staleness gate, the shape cli/0_inventory.ts already uses. Reviewable
% in a diff, readable by downstream generators without running swipl.
ruling(openapi_spec_artifact, checked_in_with_staleness_gate, user,
       'user 2026-07-30: "yea spec checked in u dinaglong"').

% 2026-07-30 openapi lab, remaining structural cards.
ruling(openapi_route_list_generated, generated_from_facts, user,
       'user 2026-07-30: "card 8 does ROUTE_LIST get generated -- yea sure". Turns parity legs 1 and 2 from a check into an identity and deletes the two-hand-kept-lists crack; it is the first production edit this arc makes to serve/').
ruling(openapi_generated_code_checked_in, spec_and_output_both_checked_in, user,
       'user 2026-07-30: "spec and produced code checked in please". Same staleness-gate shape as the spec artifact ruling').

% 2026-07-30. The null question, four candidates measured by the
% option-versus-null lab (plans/2026-07-30-option-versus-null-lab.md).
% Candidate B (Design D, T? nullable columns) is DEAD: null never enters
% storage or the type system; plans/2026-07-30-null-implementation-plan.md
% is superseded, no step of it executes. The winning shape is Datomic's:
% absence stays row absence, and the consumer that wants a total answer
% spells the default itself with get_else/2 at the use site (one body
% operator, LEFT JOIN + coalesce in SQL, `?? default` in rx). some/none
% enum variants stay available as the EXISTING per-rel variant machinery
% when the caller wants the compiler to count coverage arms; that is
% candidate A, tier-0 sugar over row absence, and it stacks with get_else.
ruling(null_design, get_else_use_site_never_storage, user,
       'user 2026-07-30: "do what makes best least brouhaha, i like none/some etc. but idk how enum wrappers and generics work here"').

% 2026-07-30. Rel-as-stream cards, settled across the session (lab =
% plans/2026-07-30-rel-as-stream-lab.md; card 4's refusal proposal was
% superseded mid-session by retention_minus making finalize-over-log fire).
ruling(stream_ordinal_spelling, seq_column_type_sugar, user,
       'card 1b: seq(name) column type, one expansion stamps the cursor rel + four rules; tier-0 (a). 1c engine-minted @ binding is DEAD: user "i HATE the @ symbol in code, its a harbinger of stupid"').
ruling(zip_reserved_row, keep_with_join_naming_message, user,
       'card 2b: user "do the least fucky thing" -- deleting the row would make a typo a silent empty EDB; the refusal message names the one-line equijoin').
ruling(stream_backpressure, watermark_gated_writer_visible_overflow, user,
       'card 3a: zero new constructs; overflow lands in a visible dropped rel instead of vanishing. User: "visible overflow being lowerable is one thing but like yea we need a way to do csp and our clock system for it i guess at some point" -- CSP = pending log + one-per-drain-tick queue + clock-joined drain, banked as a future arc, no construct known missing yet').
ruling(latest_over_log, load_time_refusal_naming_max_ordinal, user,
       'card 5b: latest() over a log rel refuses with the max(Ordinal) rewrite in the message; defining it as newest would make latest mean two things by callee decl').
ruling(stream_decl_word, no_word_convention_only, user,
       'card 6a: log + ordinal + keep bound IS the definition; a stream word would hide the retention choice, the one an author must make on purpose').
ruling(cross_rel_drain_order, non_contract_documented, user,
       'card 7a: between-rel delta interleaving is a function of drain placement, measured not-fixable-in-general by the runtime bridge arc; TICK-MODEL gets the one-paragraph non-promise').

% 2026-07-30. json-flex card C3 + dup-key card, user word.
% User proposed () for json null; () is still an ATOM, so the text value
% "()" would inherit exactly the curse "none" has today (any atom collides
% with some text). The fix is a COMPOUND term: no text value can ever equal
% a compound, so it is unforgeable from world data. Exact spelling picked
% at wiring time; json(null) is the candidate shape (bool_lit precedent).
ruling(json_null_token, reserved_ground_compound_never_atom, user,
       'user 2026-07-30: "do not use None lmfao use something else i guess, what about ()" -- () explained atom-cursed, compound accepted by delegation').
ruling(json_dup_key_fate, refuse_both_doors, user,
       'user 2026-07-30: "emitter throws if oracle throws i gues[s]" -- oracle already throws json_dup_key; emitter gains the matching refusal/guard instead of silent last-wins').

% 2026-07-30. Naming tiebreak, user word after the ordered-aggregate landing
% ("k when in doubt can we just stick to ansi or any form of a sql or sql
% standard? preferably sqlite lmfao"). Amends the vocabulary law's word pool
% (rxjs / prolog / SQL) with a PRIORITY: when candidates tie, when doubt
% exists, or when a construct's semantics live at the storage plane, take the
% SQL spelling, and specifically SQLITE's own spelling over ANSI where they
% differ (json_group_array not array_agg, group_concat not string_agg/listagg).
% The landed aggregate surface already complies (count/sum/min/max/avg/
% json_group_array/group_concat are all literal sqlite words). Does NOT by
% itself order a rename wave of the known non-SQL words (pre, keep, combine,
% finalize, now, latest -- review B8's list); those renames get their own
% user-worded arc, this ruling only fixes which way ties break when it comes.
ruling(vocabulary_tiebreak, sqlite_first_then_sql_standard, user,
       'user 2026-07-30: sqlite spelling wins on doubt; ansi/sql standard next; rx/prolog words only where the concept has no storage-plane spelling').

% 2026-07-30 night, user ruling round.
ruling(seq_sugar, approved_wire_m2, user,
       'user: "approve seq". M2 only; M1 scan and M3 stages stay unwired.').
ruling(release_gate_v620, arch_from_single_dl6_file, user,
       'user: "no release till u give me arch from single dl6 file mate, dogfood". The self-map rail must emit ARCH-MAP.md from ONE dl6 program; the python renderer must go. Push+tag gated on this.').
ruling(devlog_rail, approved_dogfood, user,
       'user: "docs YES DOGFOOD DOCS". A dl6 program reads the session ledgers and emits DEVLOG.md.').
ruling(glob_dialect, node_matcher_both_halves, user,
       'user 2026-07-31: "1->a". bind watch boot and live halves both use the node path.matchesGlob dialect (agrees with v5 globset on every measured corpus case); boot = enumerate-all + JS filter, git pathspec leaves the glob path. Fixes glob_dialect_split (170/242 corpus disagreement).').
ruling(bench_reference, proven_engine_reference, user,
       'user 2026-07-31: "2->b?". Big-scale referee = a pinned engine (tsv2 first) that EARNS reference status: byte-proven against the swipl oracle over the entire oracle-reachable corpus on every sweep; final-state hash retained as a third check at all scales. swipl stays the semantic authority where it reaches; rust graded tick-log byte-diff vs the proven reference beyond.').
ruling(type_gate_widening, arrival_gate_all_types_all_positions, user,
       'user 2026-07-31: "widen yes, do what sql would do". The decl-type refusal gate extends to all column types at all positions; coercion semantics where types CAN mix = SQLite affinity (int accepted at REAL column widens to float). Eats the bool-head card: non-boolean at a bool position = named refusal, never silent drop/coerce.').
ruling(wide_int_fate, refuse_everywhere_with_todo, user,
       'user 2026-07-31: "we dont need turbo big ints this is not finance yet lol but put todo/warn comment". int beyond 2^53-1 = named refusal int_out_of_range at every reach point incl the json-capture read-back (the bigint_seam_normalize surface); a TODO comment at the seam marks the future bigint door.').
ruling(files_naming, files_unmarked_worktree_marked_rev, user,
       'user 2026-07-31: "do not use word scan... files works fine... we want 1 that means [worktree] without a string, and the other is 1 that means a specific rev, thought we already had this figured out". Consistent with the standing worktree-unmarked spine ruling: `files(glob, ...)` = live worktree feed (no WORK atom, no string), `files_at(rev, glob, ...)` = the marked pinned case. The word scan is BANNED for file enumeration (spent twice in-tree already).').
ruling(org_fanout, repos_host_on_clock, user,
       'user 2026-07-31: "why is data from host gh org call on a timer of 1 day not enough" -- it is. Repo list = an ordinary sh host (gh org list) driven by a clock bind (1-day cadence), yielding repo(slug, root) rows; fan-out = ordinary joins + root columns on file feeds. Zero new constructs.').
