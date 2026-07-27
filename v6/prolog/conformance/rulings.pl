% rulings.pl : the ruling queue (AGGREGATE.md section 3) as facts the engine
% reads. Status: every ruling here is PROVISIONAL (coordinator took the
% aggregate's advisory to unblock the reference interpreter, user directive
% 2026-07-27 "u drive"). A user override is one fact flip + a fixture re-grade.
%
% ruling(Id, Choice, provisional | user, Receipt).

:- module(rulings, [ruling/4]).

% Q1: within-tick occurrence identity.
% hybrid = A's semantics (engine stamps (tick, seq) on event rels; order and
% multiplicity survive; folds chain per occurrence) plus the store retaining
% its IVM support count as engine bookkeeping. There is no third semantics.
ruling(q1_occurrence_identity, hybrid_stamps_plus_support_count, provisional,
       'review_occurrence_identity.md:117-135').

% Q2/Q3: occurrence scoping is an explicit rel-kind word on the declaration.
% Set = membership (identical content is the same thing, dedup holds).
% Log  = occurrence (every arrival is a new thing, stamps carried, append-only).
% Bind-filled rels infer from Stream/Tail result wrappers.
ruling(q2_scoping, explicit_rel_kind_declaration, provisional,
       'review_occurrence_identity.md:35-42').
ruling(q3_rel_kind_shape, kind_word_on_rel_decl, provisional,
       'AGGREGATE.md 1b: one word, six jobs').

% Q4: edge-written rows are arrivals for T+1, never same-tick, never dropped.
ruling(q4_edge_propagation, next_tick, provisional,
       'review_temporal_pipe.md:120-124').

% Q5: the ENGINE self-schedules drain ticks (empty outside-arrival set) while
% the carry set is nonempty; chains never freeze when outside arrivals stop.
ruling(q5_drain_scheduler, engine_owned, provisional,
       'temporal_pipe.pl:485-486 smuggled this in; now owned').

% Q6: trigger marker = explicit per-atom marker (spelled only/1 in the
% reference engine; surface spelling still surface_dcg's call). A body with
% markers fires on marked atoms only; an unmarked body keeps any-atom.
ruling(q6_trigger_marker, explicit_marker, provisional,
       'review_temporal_pipe.md:15-23').

% Q7: aggregate multiplicity = BAG of derivations (v5-SQL-compatible; two
% hits on one line count 2). The rail lab's set behavior is the rejected
% reading, kept in fixture comments.
ruling(q7_aggregate_multiplicity, bag, provisional,
       'AGGREGATE.md Q7: v5-compatibility favors bag').

% Q8: Key vs -> : both live, law stated: Key = undirected uniqueness on state
% rels; -> = the program/world column split on effect rels; det effects are
% where they coincide. (Least-commitment reading; the labs split three ways.)
ruling(q8_key_vs_arrow, both_with_stated_law, provisional,
       'AGGREGATE.md Q8 option (b)').

% Q9: count/sum/min/max (+ json_array/json_object, json arm 2026-07-27) are
% reserved head-position aggregate forms, excluded from stdlib + expr grammar.
ruling(q9_aggregate_heads, reserved_head_forms, provisional,
       'review_expressions.md:142-151').

% Q10: retention = per-rel `keep <duration|count>` clause, REQUIRED on Log
% rels; ranges over Log rels only; under q1 hybrid it is a tick-prefix DELETE.
ruling(q10_retention, keep_clause_required_on_log, provisional,
       'AGGREGATE.md Q10 option (a)').

% Residuals the engine also implements:
% R7: the tick-boundary delta set is a delta MULTISET on Log rels (one delta
%     per new stamp), a set diff on Set/level rels.
ruling(r7_boundary_diff, multiset_on_log_set_on_set, provisional,
       'occurrence store_deltas; check_eventing #1').
% equal-row keyed write = no-op (written_at column serves SWR later).
ruling(r_equal_row_write, noop, provisional, 'merge ambiguity 1').
% R1 rider: pre chains across occurrences WITHIN a tick on fold rules (the
% occurrence lab's semantics; what makes the fold correct).
ruling(r1_rider_pre_chains, chains_within_tick, provisional,
       'occurrence_identity.pl apply_occurrence').

% JSON arm (user directive 2026-07-27): json values are ordinary terms in the
% one value world (obj/list/str/int/bool/none share the struct braces), and
% aggregate heads json_array/json_object build them. See
% plans/2026-07-27-json-arm.md.
ruling(json_arm, terms_plus_aggregate_heads, user,
       'user 2026-07-27: json part of the type language, braces and all').
