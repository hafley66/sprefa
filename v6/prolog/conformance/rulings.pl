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
