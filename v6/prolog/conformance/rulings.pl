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
% R1 rider: pre/1 chains across occurrences WITHIN a tick on fold rules (the
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

% R6: pre/1 reads the EVOLVING store (T-1 exactly when nothing wrote yet;
% later occurrences chain). Frozen-snapshot pre is the rejected reading.
ruling(r6_pre_visibility, evolving_read, user,
       'the Q1 fold correctness depends on it').
% pre/2 adds only the no-prior-row value; its read ring remains R6's b -> b.
ruling(pre_seed, one_arm_folds, user,
       'yes, we should be able to ref the pre anywhere, its basically a cached let i spose').

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
% phase-C unsupported constructs (comparison/bind/head-arith) were guards against
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
% (named unsupported construct) in oracle and tsv2 both; keyed replace stays edge
% semantics. Closes the silently-inert defect from the hands-on
% findings.
ruling(keyed_level_head, named_unsupported, user,
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
% wrong-type unsupported constructs stay. The emitted runtime already canonicalized
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
ruling(json_list_one_spelling, json_list_every_layer, user,
       'user 2026-08-10: json_list(T) is the one spelling at every layer (text door, retained prolog term, emitted catalog strings); list(T) is freed for the upcoming relational generics').
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
% plans/2026-07-30-rel-as-stream-lab.md; card 4's unsupported construct proposal was
% superseded mid-session by retention_minus making finalize-over-log fire).
ruling(stream_ordinal_spelling, seq_column_type_sugar, user,
       'card 1b: seq(name) column type, one expansion stamps the cursor rel + four rules; tier-0 (a). 1c engine-minted @ binding is DEAD: user "i HATE the @ symbol in code, its a harbinger of stupid"').
ruling(zip_reserved_row, keep_with_join_naming_message, user,
       'card 2b: user "do the least fucky thing" -- deleting the row would make a typo a silent empty EDB; the unsupported construct message names the one-line equijoin').
ruling(stream_backpressure, watermark_gated_writer_visible_overflow, user,
       'card 3a: zero new constructs; overflow lands in a visible dropped rel instead of vanishing. User: "visible overflow being lowerable is one thing but like yea we need a way to do csp and our clock system for it i guess at some point" -- CSP = pending log + one-per-drain-tick queue + clock-joined drain, banked as a future arc, no construct known missing yet').
ruling(latest_over_log, load_time_unsupported_naming_max_ordinal, user,
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
       'user 2026-07-30: "emitter throws if oracle throws i gues[s]" -- oracle already throws json_dup_key; emitter gains the matching unsupported construct/guard instead of silent last-wins').

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
       'user 2026-07-31: "widen yes, do what sql would do". The decl-type unsupported construct gate extends to all column types at all positions; coercion semantics where types CAN mix = SQLite affinity (int accepted at REAL column widens to float). Eats the bool-head card: non-boolean at a bool position = named unsupported construct, never silent drop/coerce.').
ruling(wide_int_fate, refuse_everywhere_with_todo, user,
       'user 2026-07-31: "we dont need turbo big ints this is not finance yet lol but put todo/warn comment". int beyond 2^53-1 = named unsupported construct int_out_of_range at every reach point incl the json-capture read-back (the bigint_seam_normalize surface); a TODO comment at the seam marks the future bigint door.').
ruling(files_naming, files_unmarked_worktree_marked_rev, user,
       'user 2026-07-31: "do not use word scan... files works fine... we want 1 that means [worktree] without a string, and the other is 1 that means a specific rev, thought we already had this figured out". Consistent with the standing worktree-unmarked spine ruling: `files(glob, ...)` = live worktree feed (no WORK atom, no string), `files_at(rev, glob, ...)` = the marked pinned case. The word scan is BANNED for file enumeration (spent twice in-tree already).').
ruling(org_fanout, repos_host_on_clock, user,
       'user 2026-07-31: "why is data from host gh org call on a timer of 1 day not enough" -- it is. Repo list = an ordinary sh host (gh org list) driven by a clock bind (1-day cadence), yielding repo(slug, root) rows; fan-out = ordinary joins + root columns on file feeds. Zero new constructs.').
ruling(gen_word_banned, needs_rx_prolog_sql_name, user,
       'user 2026-07-31: "gen needs a new name i hate the name gen it was vibed into existence". The word gen is BANNED for the codegen-sink construct alongside scan; the templating card must propose names from rx/prolog/SQL vocabulary only (candidates to price: write/format/printf/render-class words).').
% 2026-08-03. Two or more edge arms on a log head with keep(count(N)): the
% surviving row is whichever arm ran last, and arm order is source line order.
% The alternative was a documented contract, on the cross-rel drain order
% precedent (stream_cards card 7a). Refused instead because everywhere else in
% the language moving a rule up or down is safe, and the wanted semantics
% already has a loud spelling: a keyed rel.
ruling(bounded_log_arm_order, refuse_two_arms_on_bounded_log, user,
       'user 2026-08-03: "refuse it". Named retention_head_conflict_risk(HeadRef, count(N)), sibling of the keyed edge_head_conflict_risk at analyze.pl:1350 and broader than it: no shared-trigger condition, because retention prunes at tick end rather than per occurrence, so arms on different triggers still collide. Covers count(N) for every N, not only count(1). Measured before ruling: ZERO tracked programs carry the shape, so the unsupported construct breaks nothing.').

ruling(repo_column_spelling, distinct_name_hosts, user,
       'user 2026-07-31: "we want A bc no magic strings repeated all the time since we dont have defaults or nulls" -- repo-scoped enumeration is its own host pair (repo_files(repo, glob, ...), repo_files_at(repo, rev, glob, ...)) beside the unscoped files/files_at, never a required leading column with a repeated cwd literal. Follows the repo_grep_at precedent and the no-defaults/no-nulls design line: a coordinate the program does not vary is a host the program does not name.').

% 2026-08-03 evening: the laziness fork menu, ruled via drawn options.
ruling(pulse_merge_spelling, edge_arms_with_latest_sample, user,
       'user 2026-08-03: canonical merge-of-pulses = edge arms with an explicit latest/1 sample of the latch (compiles today, grade 0). Level-accumulate stays legal; the bare-atom edge spelling stays refused (the grade rule at 3_clock_check.pl:129-138 is untouched).').

ruling(subscribed_reset_pole, per_rel_declaration, user,
       'user 2026-08-03: reset behavior is per-rel. Default = rx share(), cold when the last reader unsubscribes; a rel opts into warm (never re-cold) explicitly. The pre-commit example global is one warm opt-in, never a global default. Decoupling running from the ticks$ refcount is a defect fix independent of this pole.').

ruling(edge_before_first_subscribe, keep_table_is_the_replay, user,
       'user 2026-08-03: an edge rel ingests eagerly into storage; evaluation is lazy. A late subscriber reads the keep()-bounded table then the live stream (concat(from(storedRows), live$)). No second ingress buffer mechanism.').

ruling(event_ingress_surface, live_event_bind, user,
       'user 2026-08-03: outside-world events enter as a new bind name, live_event, whose executor starts no process; rows arrive through POST /edb/events and type-check like any arrival. No new keyword (external/register died; bind is the survivor).').

ruling(zero_query_semantics, subscribes_nothing, user,
       'user 2026-08-03: "im sorry are u saying if no file has a question in it we just subscribe to everything?" -> strict: a program with no query subscribes to NOTHING. The conformance harness seeds subscription roots from its own expectations (zero fixture edits). The compat all-rels branch in the cone is a migration bridge scheduled for removal.').

ruling(subscribe_vocabulary, subscribe_never_demand, user,
       'user 2026-08-03: "and call it subscribes damn it". The cone family uses subscribe vocabulary everywhere: subscribed_rels/4, subscribedRels, 2_subscribe.pl. The word demand is banned for this construct family in surface, identifiers, and docs (host __host_demand_* rows are a separate pre-existing family, rename not ruled).').

% 2026-08-03 night: the one() fork, ruled after the compose lab's four-run race
% table (COMPOSE.md section 3): oracle referees by arrival order, emitter by
% source arm order, and they agree only when the orders coincide.
ruling(one_pick_order, arrival_order_per_tick, user,
       'user 2026-08-03: "i simply want arrival time, who got there first for that tick, congrats, thats the winner". The pick inside a tick reads the arrival index, on BOTH doors. Source arm order is not an axis of the clock; the emitted module must stop consulting it (today it runs concat-of-arms where the oracle runs merge). Rx: merge(arms).pipe(groupBy(key), mergeMap(g => g.pipe(take(1)))), never concat.').

ruling(one_admission_no_lockout, both_folds_stay_sound, user,
       'user 2026-08-03: "we cannot lock ourselves out of 2 choices that are both sound". First-wins (scan((acc,row) => acc ?? row), state==history, log-vs-key vacuous) and one-per-tick takeover (zip(perKey, ticks$), keyed, each admission replaces visibly) are BOTH sound constructs; ruling one() = first-wins never forecloses the serializer spelling. Neither is built yet; the design lane prices both.').

ruling(one_decl_surface, rel_declaration_only, user,
       'user 2026-08-03: "i dont want a non rel declaration feature fucks with the other features bc muh constraint soundness". Whatever one()/admission becomes, it lands as a rel-declaration property beside key(1) and log keep() so the existing decl checkers (edge_head_conflict_risk, retention_head_conflict_risk, type gate) see it natively; a freestanding construct class with unpriced decl interactions is refused at the design stage. Standing note: the keyed-vs-log split itself is disliked and will be revisited; no new feature may deepen the split before that revisit.').

% 2026-08-04 morning: the tick boundary, ruled after the v8/event-loop survey
% (macrotask = tick, microtask drain = rounds, boundary = queue exhaustion;
% v8 owns no clock and neither do we).
ruling(tick_boundary, ingress_transaction_list, user,
       'user 2026-08-04: "make the events that are in-tick be a list of events, and its usually a list of one". A tick dequeues ONE ingress transaction = an explicit list of events, list of one in the common case. Simultaneity is opt-in: it exists only when the submitter deliberately batched (one file save, one commit, one schedule row), never manufactured by the engine coalescing independent sources into a shared tick. Consequences: same-tick multi-writer conflicts shrink to refereeing DELIBERATE batches (the one/any family scope-cut); independent contenders land on successive ticks automatically (the deferral door happens by construction); the engine surface already matches (submit takes IArrivalBatch, concatMap runs one batch per tick, 3_engine.ts:104); the constraint binds every future ingress path (live_event, bus, clock binds): one submission = one tick, no auto-coalescing.').

% 2026-08-04 midday: the duel words, ruled. Ends the throttle-vs-zip fork
% (plans/2026-08-04-rxprim-duel-verdict.md word 1).
ruling(admission_word, lossless_queue_concat_family, user,
       'user 2026-08-04: "no dropping events, exhaustMap is not what i want, this is concatMap territory, idk why its zip but dont lose info". The reserved admission door = LOSSLESS QUEUED admission: one admission per key per tick, remaining contenders WAIT for successive ticks, nothing is dropped. Drop-flavored spellings (throttle, exhaust) are REJECTED for this construct; zip is rejected as the WORD while its lockstep semantics survive; the surface spelling comes from the rx concat family, exact form priced in the fuse contract. one_pick_order (within-tick pick = arrival order) is untouched: it referees who is FIRST in a deliberate batch, this ruling says the rest queue instead of vanishing.').

% 2026-08-04 midday: design-debt mode declared. Language design is FROZEN in
% favor of shipping ghcacher, the golden use cases, and full sprefa-extract
% usage. Any AI-taken design decision during this mode carries the user's
% banner verbatim at the decision site.
ruling(design_debt_mode, utility_over_pedantry, user,
       'user 2026-08-04: "im done language designing, do whatever it takes to be able to competently express ghcacher and our golden usecases and full usage of sprefa-extract. stick a fork in it and accrue lang design debt fuck it, mark it accordingly with big fucking loud letters: FUCKING WARNING: AI IS DUMB AND DECIDED THIS BECAUSE USER IS WANTING SOMETHING USEFUL AND IS DONE BEING PEDANTIC, DONT CAUSE CONTRADICTIONS WHEN POSSIBLE BUT FUCK IT". Debt decisions taken under this banner so far (ghcacher plan Q1-Q3): host concurrency stays serial concatMap (knob deferred); rate back-off = relational pause (over_budget anti-joined into due), never a sleeping host; change detector = one conditional org-events call, with the live smoke leg measuring whether a private org blinds it before any lane depends on it. Each lands in code/docs with the banner.').

% 2026-08-04 midday: block sugar timing, ruled (duel word 2). The lowered form
% is the construct; braces come later as sugar over it.
ruling(block_lowering_first, flat_rels_catalog_edges_arg_distribution, user,
       'user 2026-08-04: "if a file is our first block syntax its not really sugar anymore... make a middle of the road abstraction that we open later... relate rels to each other after we lower them into longer names and if we capture arg from outside world its implicitly captured, distribute that arg into every thing, that sounds sugarable". Block construct v1 = the LOWERING: children land as flat rels with long mangled names (module-catalog M5 spelling) plus catalog rows relating them; an outer arg the block captures is IMPLICITLY DISTRIBUTED into every child rel as a leading demand-key column (module-catalog M1, data-driven scalar args). The brace surface is sugar over that lowering and arrives in a later wave; a FILE is the degenerate first block already. Consistent with modscope decisions 7 (module = rel/0 with children), 8 (dotted heads contribute), 10 (block-under-rel = extension surface).').

% 2026-08-05: type-IR step g catalog universe, decided. Ends the A-vs-B fork
% (opus verdict .agent/salvage-20260805/GWORD-OPUS-VERDICT.md, flash counter PR #4).
ruling(catalog_universe, user_rel_decls_in_program_db, user,
       'user 2026-08-05: "i want to be able to read the rels as values/types/mods whatever as their own types with dots... A sounds bad all around, they should always query, we can also host things as well". Step g catalog rows describe USER-PROGRAM rel declarations, produced from the compiler decl table (relplan, emit_ts.pl:656-703), materialized into the COMPILED PROGRAM db via the same door as __tick tables (lower.pl:622-628). The store-spine alternative is rejected: the v6/dl fact plane and a compiled program are separate databases with no ATTACH anywhere (scratchStore.ts:1-11), so spine catalog rows are unreachable by the user rules the catalog exists to serve. Dot access over rels resolves against these rows. Hosts may feed catalog rows too where a producer outside the compiler is the natural source.').

% 2026-08-08: effect/demand declaration surface, ruled. NO decl arrow family.
% One relation, datalog convention: response is the rightmost column(s);
% demand columns spelled by Key()/convention; effect-ness comes ONLY from a
% bind existing at link time. LANG.md's `rel f(a) -> B` spec line is dead;
% parse_dl never implemented it (parse_dl.pl:1030 "not acquire another arrow
% family" now covers rel decls too).
ruling(effect_decl_no_arrow, one_relation_rightmost_response, user,
       'user 2026-08-08: "nah nvm on it, just do the datalog way and have return be the right most value and keep 1 relation". Supersedes the LANG.md arrow spelling; resolves open items "Key(Type) vs ->" and Q8 left-of-arrow residual.').

% 2026-08-09: catalog fork F5, ruled A. The oracle goes meta: conformance/
% ticklog.pl mints catalog rows so a fixture can read __rel and still grade.
% Closes catalog_g2 as written (5_compiler_quality.pl:249-252 recorded the
% old impossibility; that record is now a work item, never a wall).
ruling(catalog_oracle_meta, ticklog_mints_catalog_rows, user,
       'user 2026-08-09: "skill issue make it meta and allow it. stop taking refusals or \'i cant do this bc code said it\' as fact, the code is wrong, why do you think we are still working on it". F5 = A: the oracle models the compiler meta plane too, a compiler-owned table in the oracle is allowed. Restates the standing law: a code-encoded refusal is a hypothesis, never an edict.').

% 2026-08-09: option(T) surface, ruled (three answers, one word each).
ruling(option_surface, both_spellings_per_instance_none_per_element_enum, user,
       'user 2026-08-09: (1) BOTH spellings legal, option(text) and text?; (2) none is PER-INSTANCE, some/none ids never compare across different option types; (3) desugar mints ONE enum per element type (__opt_text style, dictionary reuse), never one per column site. Design doc plans/2026-08-08-option-type-design.md; unblocks the option implementation lane.').

% 2026-08-09: catalog fork F1, ruled A. Plane rows DO land in the emitted TS
% const; the host-app type system sees mode/departures (insert-into-derived
% becomes a compile-time error surface). Regen churn declared a non-cost by
% the user. Execution stays SQL-first through the ladder's zero-diff steps;
% the const widening is one deliberate final step with a re-pin.
ruling(catalog_plane_in_const, plane_rows_emitted_to_ts_const, user,
       'user 2026-08-09: "idgaf about regen-ing files when we add new things, its kinda the point of coding" then "A is fine". F2''s byte-neutrality justification becomes scaffold-only; the widening step may grow catalog_rows to /8 or render both halves.').

% 2026-08-09: type-IR arc 2, ruled 1. The parallel decl records merge into ONE
% kernel record and each column says where its type came from.
ruling(type_ir_three_slot, one_rel_record_col_name_origin_storage, user,
       'user 2026-08-09: "lets go 1 yes rx.merge all into 1 and mark where they came from/are, and we can use this as our kernel for all things". The record is 0_rel_record.pl rel(Ref, Kind, Cols, KeyOrNone) with Cols = col(Name, declared(WrittenType)|inferred, Storage); relplan/5, and the separate col_type/keyed/kind reads downstream of `plan`, are gone. Declared and inferred both survive because they answer different consumers: column_def/4 reads Storage at every column, the arrival gate reads declared only (decl-driven on both doors, ruling type_gate_widening) and stays all-or-nothing per rel. type_def/3 stays a SIBLING: column_storage/3 needs the type table to fill this record''s Storage slot, so the type table must exist before the first record does. Checks-first ordering is unmoved, pinned by fixture key_range_reported_before_unknown_column_type on both doors.').

% 2026-08-09: annotations, ruled @ with auto-curry. An @ annotation is an
% erasing wrapper: zero effect on storage, unification, h_schema; payload
% lands as catalog-plane rows keyed to the annotated node. Record before
% erase (the option(text) surface-spelling loss in the kernel record is the
% named counterexample).
ruling(annotation_at_curry, at_wrapper_rows_auto_curry, user,
       'user 2026-08-09: "yes for at symbol, it auto curries and can have types itself described in our lang but the currying will be a different concept we may use elsewhere in the lang but we could dip our feet with just @ symbols basically just being like a prolog list that is not checked except for its own at comptime". MVP scope: @ann(args) parses before decls/cols as an unchecked payload row, typed only against the annotation''s own in-language decl at comptime; @f(a)(T) desugars to the 2-arg parens form; currying generalizes later as its own concept.').

% 2026-08-09: mount fork 1, ruled additive. `use "x" as u` is a soft link:
% bare names splice in alongside the alias, both spellings resolve.
ruling(mount_alias_additive, alias_soft_link_bare_names_stay, user,
       'user 2026-08-09: "additive lol allow soft linking/alias assign that is fine". No exclusive mode; an alias adds a path, never hides one.').

% 2026-08-09: mount fork 2, ruled no leak. An inner alias is private to the
% module that declared it; visibility outward takes an explicit re-export
% (mainstream convention: rust pub use, es export-from). Priority LOW; the
% user-stated deliverable is the module graph as rel-to-rel rows for HMR and
% async loading, reusing existing graph algos, and that ships first.
ruling(mount_inner_alias_private, inner_alias_no_outward_leak, user,
       'user 2026-08-09: "no" to mid''s grove being visible from main via mid''s alias; "i just need to be able to input the module graph rel to rel relations for hmr and async loading and re-use as many graph algos as we can". Fix rides a MOD wave, not urgent.').

% 2026-08-10: boop-in-dl6 shape. boop enters dl6 through sh host decls (the
% existing sh door); TypeScript stays the core engine. Rust emitters are a
% later arc, and only then does dl6 link boop natively.
ruling(boop_dl6_sh_door, sh_hosts_now_ts_core_rust_emitters_later, user,
       'user 2026-08-10: "boop stays as sh code in dl6 for now, ts is the core engine and when we get far enough to factor it into rust emitters, then we can get there and link into our homies". Bridge item 7 (boop base facts to DL6) therefore lands as sh decls calling the boop CLI, never a bespoke native bridge.').

% 2026-08-10: mount fork 3, ruled NO cycles for now. Reversed same day: the
% first call was allow-cycles ESM-shaped (lazy cross-cycle refs legal, only
% an eager top-level ?- read mid-load errors); the user withdrew it before
% any code moved. The on-stack throw at use_resolve.pl:95 stays, pinned by
% plunit use_cycle_refuses_naming_the_chain. If cycles ever open, the ESM
% shape above is the recorded design sketch: memo dedup on loaded/2 replaces
% the throw, module_hash needs SCC-as-a-unit hashing.
ruling(mount_mutual_cycles_deferred, use_cycle_throw_stays_esm_sketch_parked, user,
       'user 2026-08-10, first: "i would prefer if we allowed cycles like how js does it, where it does not care if u use something as a reference that is not reachable from module load traversal tick/pass, so if a subscribe happened like a query at top level (i think ?- or whatever it is is effectively .subscribe()) then that would yell at you"; then: "hmm i dont want cycles fuck it no cycles ... at least not yet".').

% 2026-08-10: export signifier, ruled pub. Re-export and outward visibility
% spell rust-style: `pub use` re-exports an inner mount through the module
% boundary (the mount_inner_alias_private mechanism gets this spelling).
% Bare rels stay all-public to the direct consumer (mount_alias_additive);
% a declared-public default flip was NOT taken. Surface is unbuilt: zero
% pub/export hits in compile/parse_dl.pl at decision time.
ruling(export_signifier_pub, pub_use_rust_spelling, user,
       'user 2026-08-10: "just use pub semantics i guess", closing the fork-2 spelling question (mechanism decided earlier as explicit re-export, no outward alias leak).').

% 2026-08-10: @ is not a macro escape. The user refused making @ an
% arity-driven exclusion from the language in the way rust quarantines
% variadic shapes behind vec!-style macros. Currying and lists ARE
% quarantined for initial use, for one stated reason: RHS types stay
% symmetric. Generics arrive as comptime parens (parenthesized comptime
% parameters), not a separate macro layer.
ruling(at_not_macro_escape, curry_lists_quarantined_generics_comptime_parens, user,
       'user 2026-08-10: "i dont want @ to be something we make as exclusion to language like rust and vec! for arity sake, but currying and lists are to be quarantined for initial use for the reason that we want symmetric rhs types, our generics will be comptime parens".').

% 2026-08-10: list generic surface. Named constructors ship as the lab built
% them; an options grammar for list flavors is deferred. list(T) is the bare
% constructor, distinct from the three named flavors.
ruling(list_surface_named_constructors, named_constructors_options_deferred, user,
       'user 2026-08-10: "D4 = C, named constructors as the lab built them, NO options grammar now".').

% 2026-08-10: bare list(T) default combo. A bare list(T) column lowers to the
% relational dense+owned+sequence flavor: one list entity plus its member
% table, referenced by the entity id column.
ruling(list_bare_default_dense_owned_sequence, bare_list_is_dense_owned_sequence, user,
       'user 2026-08-10: "Bare list(T) = relational dense+owned+sequence (the lab''s default combo)".').

% 2026-08-10: list flavor set v1. Four constructors ship: list(T),
% list_entity_dense_sequence(T), list_interned_set(T), list_entity_linked_sequence(T).
% json_list(T) is the inline-JSON spelling and does not collide with list(T).
ruling(list_flavor_set_v1, four_lab_constructors, user,
       'user 2026-08-10: "Ship the lab''s four constructors: list(T), list_entity_dense_sequence(T), list_interned_set(T), list_entity_linked_sequence(T)". json_list(T) is the inline-JSON term at every layer (main 1d0e294a).').

% 2026-08-11: dl6 source formatting is multiline except for declarations and
% simple facts with at most two terms.
ruling(dl6_formatting, single_line_only_decls_and_2term_facts, user,
       'user 2026-08-11').

% 2026-08-10: author column order remains program data through expansion.
ruling(decl_order_fix_a, author_column_order_is_data, user,
       'user 2026-08-10: fix A').

% 2026-08-10: JS is never the row engine. The 4a9b45f7 incident (failure-modes
% entry 45) exposed boundary_delta materializing a 1,069,200-row delta into V8
% row-by-row; the user ruled the whole shape out, not just the incident. rxjs
% is the tick/flow boundary ONLY; row work stays in emitted SQL; anything
% SQLite can compute (consolidation, aggregation, checksums) the emitter must
% emit as SQL, and rows cross into JS only at a true app-boundary subscriber,
% never as an engine step.
ruling(js_never_the_row_engine, rows_stay_in_sql_rxjs_is_flow_boundary, user,
       'user 2026-08-10: "we should truely never be using js as sqlite lol, yes, do not materialize rows mate, rxjs is the cut time boudnary for flow, okay. if sqlite can do it, our emitter should predict that".').

% 2026-08-10: template rules policy. Generic templates mint DECLARATIONS only;
% no maintained rules or guards are generated per instance. Operations over
% minted tables are author-written. Opt-in library rules stay an open road.
ruling(generic_template_rules, declarations_only, user,
       'user 2026-08-10: "b" to decls-only vs template-minted rules vs opt-in library rules; unused maintained heads are write amplification per sqlite-costs; upgrade path to opt-in library rules stays open').

% 2026-08-10: payload-enum variant fields may name a declared relation as
% their type, same as any plain column (oneOf payloads reference relations).
ruling(enum_variant_rel_payload, variant_fields_can_ref_relations, user,
       'user 2026-08-10: dispatch "fix/enum-rel-payload": rel-typed variant payload fields in payload enums; oneOf mapping needs variant payloads to reference relations').

% 2026-08-14: template bound surface spelling. Bounds live inside the existing
% parameter parens; angle brackets stay out of the grammar entirely.
ruling(template_bound_spelling, bound_in_parameter_parens, user,
       'user 2026-08-14: "parens all the way, i dont want antisymmetry" — rel pair(T: json_encodable)(first: T, second: T). over <T: ...> and where-clauses; parens are the one grouping symbol').

% 2026-08-14: acyclic surface spelling. Wrapper composition (spelling A in
% plans/2026-08-14-acyclic-spelling.PLAN.md), default-on: a column typed
% option(<the declaring rel>) carries the arrival-time chain walk with no
% syntax; acyclic(option(node)) is the explicit synonym for the same guard.
ruling(acyclic_guard_spelling, wrapper_composition_default_on, user,
       'user 2026-08-14: "A for now then". acyclic(option(node)) wrapper composition wins over a rel-level clause or a stdlib template; earlier in the same session "no, detect it" set divergence detection over any hang.').

% 2026-08-16: TOML ingestion. No new reader dependency and no awk template;
% TOML (and yaml) read through the format-polymorphic json operator exactly as
% v4/v5 did (v5 `json`/`jsonp` ops dispatch on file extension in datapath.rs).
% Closes the ghcacher plan's open question 4 (plans/2026-08-04-ghcacher-plan.md).
ruling(toml_via_json_operator, format_polymorphic_json_op, user,
       'user 2026-08-16: "toml is read by json operator im not relitigating this read v4/v5 json operator" — the v6 json op inherits v5 format polymorphism; no dasel/yj/tomlq, no awk').

% 2026-08-20: the prolog oracle is demoted from per-pass executor to snapshot
% minter. Sweeps diff emitted-door outputs against the COMMITTED oracle
% snapshots (out/*.oracle.jsonl); the oracle re-executes only a fixture whose
% program, schedule, or engine.pl/ticklog.pl digest changed, and on demand for
% a named algorithm comparison. Snapshot regeneration is a reviewed diff.
ruling(oracle_demoted_to_snapshots, snapshots_are_the_truth_between_semantic_changes, user,
       'user 2026-08-20: "idk why i have 2 impls its fine to oracle specific algorithms comparison but it feels like extremely dead weight/low gain considering how far we are on snapshots that the oracle was involved in". The oracle stays the source that MINTS an expected output (a new construct still gets its snapshot from the oracle once), and stays available for targeted algorithm comparisons; it stops re-executing all 461 programs every sweep pass. Cross-door protection is preserved: TS and Rust doors still both diff against the same frozen snapshot, and grade.sh byte-clean still pins door agreement. Mechanism = the sweep digest cache (perf/sweep-shard lane) keyed on fixture + engine digests, applied to stage 2.').

% 2026-08-20: amendment to oracle_demoted_to_snapshots. The oracle stage is OFF
% BY DEFAULT (SWEEP_ORACLE=1 opts in); default sweeps diff frozen snapshots
% only; a fixture without a snapshot fails loudly naming the mint command.
ruling(oracle_off_by_default, sweep_oracle_env_opt_in, user,
       'user 2026-08-20: "literally default the prolog sweep to false bc the conformance prolog is literally not my product". The reference prolog exists to mint snapshots and settle named algorithm disputes; it is not a per-pass gate. Wiring lands via perf/oracle-grind.').

% 2026-08-21: the one-rel-with-arrivals collapse (user, verbatim: "all the old
% bind and sh and host shit is now just arrival and ticks", "send rel''s in and
% out of the thing"). The arrivals-and-ticks brief instructs the lane to close
% the plan''s section-13 forks by the reading most consistent with "a rel whose
% rows arrive from outside is still just a rel". The four picks:

% Fork 1, the arrow. A parenthesized COLUMN group after `->` on a plain rel
% declaration is the response column list of an arrival rel; the group is
% recognized by its `ident :` prefix, so anonymous SUM literals `(Ok(..); ..)`
% and arrow types `((a: int) -> int)` keep their spellings, and the anonymous
% PRODUCT keeps its column-position spelling `rel r(result: (a: int, b: text))`.
% One authored line moves (v6/dl/fixtures/anonymous-type-syntax.dl6).
ruling(arrival_arrow_spelling, paren_group_after_arrow_is_response_columns, user,
       'arrivals-and-ticks brief 2026-08-21: `rel n(ins) -> (outs) key(..)` is the one arrival form; the anonymous product loses arrow position (one authored line) and keeps column position. Desugars to sh_decl(N, Ins, Outs, template(\'\')) so every later phase and emit_ts.pl see the term they saw yesterday.').

% Fork 2, key() doing two jobs. On a stored rel key(..) is UNIQUE positions;
% on an arrival rel (arrow present) it is demand identity: the named INPUT
% positions identify the answer, every other input is freshness. Same word,
% two readings, held apart by the arrow: an arrival declaration desugars its
% key() into arrival_identity/2 and mints NO keyed/2, so the storage reading
% never sees it. A rel is still just a rel; key() states which columns say
% WHICH rel row the world is answering.
ruling(arrival_identity_spelling, key_positions_are_demand_identity_on_arrival_rels, user,
       'arrivals-and-ticks brief 2026-08-21: key(P..) on an arrival rel = identity inputs, rest freshness; absent key() = all identity (registry identity_roles/2 unchanged). host_input_contract/3 rows stay as the term-door fallback.').

% Fork 3, the keywords. `sh` and `bind` die from the surface now, not later:
% the parser accepts only the rel form, and every authored program moves in
% the same arc. The TERMS sh_decl/4 and bind-shaped schedules stay internal,
% which is what keeps emit_ts.pl untouched and its output for unchanged
% programs byte-identical. Templates die with `sh`: linked executors read
% named inputs, never a command line, so the surface carries no backtick
% string and validate_template/4 guards on the non-empty templates the term
% door may still carry.
ruling(sh_bind_surface_removed, arrival_rel_is_the_only_spelling, user,
       'user 2026-08-21 verbatim: "all the old bind and sh and host shit is now just arrival and ticks". bind interval/watch included; /clock/tick and /soopy/watch are arrival rels answered by continuing executors, and a continuing executor''s re-answer is a tick.').

% Fork 4, the fixture executor. A fixture answer is rows arriving from
% outside, so it travels the arrival paths that already exist (--arrive and
% the schedule file), never a template-parsing executor. FixtureExecutor and
% its printf template die with the surface.
ruling(fixture_answers_are_arrivals, schedule_and_arrive_replace_fixture_executor, user,
       'arrivals-and-ticks brief 2026-08-21: a rel whose rows arrive from outside is still just a rel; a canned answer is an arrival batch, not an executor.').

% Executor namespacing, same brief: every rel that reaches an executor carries
% its executor family, and the family is named by an import (`use soopy.`),
% never left implicit. "No bare files, no bare fetch" means no bare files
% WITHOUT a use. The registry''s arrival_executor/2 rows are the one roster;
% LINKED_EXECUTORS in sprefa-engine-rs/src/hosts.rs lists the same names and a
% test pins the two equal. An arrival rel no executor links (a replay-only
% fixture feeder) keeps a plain name and no family binds it.
ruling(executor_namespacing, dotted_executor_question_names_registry_is_roster, user,
       'arrivals-and-ticks brief 2026-08-21: "non confusing named and well namespaced"; no bare files, no bare fetch. The __ atom join (module_path_name/2) stays the internal spelling.').
% Key is a column annotation: `rel files(glob: key(text)) -> (path, digest)`.
% `key(int)` is the type relation `rel key(Target: type) -> Target`
% (compile/test/annotation_surface.test.pl:26, dl/fixtures/0_typespec_basic_probe.dl6:25).
% The suffix form `) key(1)` (parse_dl_dcg.pl:863 key_clause) is deprecated:
% no new program writes it, the one-rel collapse rewrites what it touches.
ruling(key_column_annotation_over_suffix, annotation_preferred, user,
       'chat 2026-08-21: "key is annotation generic now, key at suffix special position is defunct"').

% An executor family is a MODULE. `use soopy.` then bare `files(...)`, or
% `use soopy as sy.` then `sy.files(...)`. `rel soopy.files(...)` and
% `rel /soopy/files(...)` still parse and mean the same rel; no file in this
% repo writes either. The DECLARATION is what binds: `use soopy.` plus
% `rel files(...)` makes that rel soopy's, so a file wanting a `files` of its
% own aliases the module instead. Two used families exporting one leaf, both
% unaliased, stop at ambiguous_executor_leaf. The internal __ atom join
% (module_path_name/2) is unchanged, so every emitted name is byte-identical
% across the three spellings (executor_modules.pl, compile/test
% plunit unit executor_modules).
ruling(executor_modules_use_import, use_named_module_then_bare_leaf, user,
       'user 2026-08-22, in order: "Put the dots back; keep the slashes as an alias for dot; never use them." then "Dont require dots either. You should be able to import things with an alias or by module name."').
% The clock checker's path walk (clock_path_conflict, unconstructive_clock_cycle)
% is PINNED OFF the compile path: it was early-stage and nothing can express
% infinite yet. The cheap cross_plane checks stay. The code stays, commented as
% the seed of a later calculus: edge reference counting (when a full retraction
% invalidates other edges and refCounts, auto-drop like Rust), relational
% cardinality over time, and det modes in the Mercury sense with clocks on the
% pipeline ("this pipe is lazy 1"), to catch rxjs-world bugs at compile time.
ruling(clock_path_check_pinned_off, stub_keep_cross_plane, user,
       'chat 2026-08-21: "fuck the clock checker at this point, pin it ... comment it/stub it out, we havent a need for it yet bc we cant even express infinite yet"').

% Shell comes back ONLY as an ordinary executor rel, never a keyword:
% `rel /sh/run(cmd: key(text), cwd: text) -> (line: int, out: text, status: int)`.
% The `sh` declaration form stays dead.
ruling(shell_as_executor_rel, rel_sh_run_only, user,
       'chat 2026-08-21: "we will add sh back but it will literally be rel sh(cmd: string, etc.)"').

ruling(per_rel_delta_only, no_program_wide_recompute, user,
       'chat 2026-08-23: "yea im trying to efficiently compute dbsp, not spamming its worst versions" and "only do work for what has its deps changing". A rel does work only when a rel it reads moved; a `<+` arm that reads pre/1 is sequenced per occurrence, per ARM, never by flipping the whole module onto a rebuild loop (emit_rust.pl:216 ordered_program/1 is the defect). Plan: plans/2026-08-23-one-tick-path.PLAN.visual.human.unga.md.').
