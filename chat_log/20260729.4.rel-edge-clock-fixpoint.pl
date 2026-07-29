% Consultable session ledger for the relation-edge and clock fixpoint.
% Load:
%   swipl -q -l chat_log/20260729.4.rel-edge-clock-fixpoint.pl
% Useful queries:
%   goal(Name, State, Contract).
%   locked(Name, Evidence).
%   hole(Name, Evidence).
%   iteration(N, State, Contract).
%   verification(Name, Result).

:- module(session_20260729_4, []).

:- discontiguous observed/2.
:- discontiguous touched/2.
:- discontiguous verification/2.
:- discontiguous next_action/2.
:- discontiguous locked/2.
:- discontiguous leading_hypothesis/2.
:- discontiguous closed/2.

session(id, '20260729.4').
session(date, '2026-07-29').
session(branch, 'codex/rel-ref-file-span-lab').
session(base_commit, '592d23a3').
session(topic, rel_edge_clock_fixpoint).

goal(minimal_orthogonal_kernel, in_progress,
     'grade the rel-only entity/edge model against current SQLite lowering and Rx tick semantics without adding surface constructs').
goal(keyed_reference_identity, in_progress,
     'make existing key metadata control relation-edge target resolution for single and composite keys').
goal(reference_clock_contract, in_progress,
     'characterize target arrival, missing target, keyed replacement, retraction, and derived-edge behavior at exact ticks').
goal(ref_proof, in_progress,
     'attempt opaque row identity capture and transport through current compiler mechanisms before selecting ref syntax').

locked(one_declaration_family,
       'surface type keyword removed; rel declarations remain public and queryable when referenced').
locked(reference_is_edge,
       'a relation-domain parent column physically stores the dense row id of one target relation row').
locked(identity_policy,
       'existing key(...) defines entity identity; an unkeyed set uses its full row identity').
locked(no_json_value_plane,
       'typed relation edges use ordinary target tables and integer endpoints; no dictionary table or stored semantic/rendered JSON').
locked(file_span_shape,
       'file_span(rev_file,start,end), key(rev_file,start,end); analysis facts store file_span_id').
locked(optional_capabilities,
       'git_blob and stored_blob are additive relation memberships; absence requires no NULL payload').
locked(host_shape,
       'external work attaches behind ordinary demand/response relations and must batch by repository and blob').

hole(key_not_driving_reference_resolution,
     'runtime relation-value interning still keys by canonical full JSON and ignores declared key positions').
hole(relation_constructor_emits_json,
     'a relation-shaped constructor in a relation-domain head column lowers to JSON text for INTEGER storage').
hole(keyed_cycles_refused,
     'type_cycle_witness applies the content-DAG refusal to keyed entity graphs').
hole(opaque_identity_capture,
     'a standalone RHS target scan exposes fields but cannot bind the hidden dense identity').
hole(clock_receipts_missing,
     'no paired oracle/emitter fixture currently pins missing target, target replacement, target retraction, and parent edge timing').

% Every iteration uses the real parser, analyzer, lowerer, emitted SQLite
% runtime, and oracle where the oracle still represents the selected model.
iteration(0, completed,
          'remove type surface, keep referenced rel public, replace dictionary schema with ordinary target table plus hidden __id').
iteration(1, completed,
          'measure file_span, path, content, cache, and string-domain storage on extractor output').
iteration(2, in_progress,
          'make key positions drive automatic relation-edge construction and separate entity cycles from content cycles').
iteration(3, pending,
          'pin missing-target, replacement, retraction, antijoin violation, and exact tick behavior').
iteration(4, pending,
          'test opaque identity capture and transport; register ref only if current variables/modes cannot express a required case').
iteration(5, pending,
          'run one extractor-to-file_span-to-text/line/column vertical slice through the batched provider boundary').

clock_hypothesis(target_arrival,
                 'target row resolution occurs before parent write in one transaction; no parent endpoint reaches a missing target').
clock_hypothesis(keyed_replacement,
                 'replacing non-key target fields preserves semantic entity identity and therefore preserves parent endpoints').
clock_hypothesis(target_retraction,
                 'ordinary relation retraction changes target membership; parent edge lifetime follows explicit support and no SQL cascade is implied').
clock_hypothesis(missing_target_diagnostic,
                 'a dangling-edge invariant is an antijoin and retracts automatically when the target appears').
clock_hypothesis(provider_response,
                 'cold demand commits before async provider response; cached response participates as current relation membership').

next_action(1,
            'add fail-first actual compiler fixtures for single/composite key resolution and relation constructor SQL').
next_action(2,
            'change the shared relation-row resolution path so arrival and derived construction use declared keys').
next_action(3,
            'split content-cycle and keyed-entity-cycle checks using existing key declarations').
next_action(4,
            'add exact tick fixtures for target and parent arrival/retraction order').
next_action(5,
            'record measured statement count, query plans, sweep disagreements, and remaining cracks in ARCH and this ledger').

verification(branch_checkpoint, passed('592d23a3')).
touched('chat_log/20260729.4.rel-edge-clock-fixpoint.pl',
        'this cross-session fixpoint ledger').
touched('v6/prolog/ARCH.pl',
        'rel_edge_clock_fixpoint task, locked model, iteration order, and exit contract').
touched('v6/prolog/labs/rel_value_unification/8_key_edge_case_census.pl',
        'twelve actual compiler checks for key DDL, replacement/retraction asymmetry, cycles, invalid positions, and current JSON construction').

observed(key_edge_case_census,
         '12 checks pass against the current parser, plan, and lowerer').
observed(key_ddl_ready,
         'single and composite key positions already become UNIQUE constraints on public referenced target tables').
observed(key_write_asymmetry,
         'positive keyed arrival uses INSERT OR REPLACE by key; negative arrival deletes by every row column, so a stale old-row retraction does not remove its replacement').
observed(key_position_validation_hole,
         'key(0), key(arity+1), and duplicate key positions all survive plan construction').
observed(key_cycle_overreach,
         'single and mutual keyed entity cycles are both rejected by the inherited content-DAG check').
observed(key_constructor_hole,
         'typed reference construction still consumes the full relation arity and emits JSON despite key DDL already existing').

iteration_gate(2, valid_key_positions,
               'reject zero, out-of-range, and duplicate positions before DDL or runtime code generation').
iteration_gate(2, replacement_retraction_clock,
               'pin whether stale exact-row retraction is the intended occurrence contract for keyed entity state').
iteration_gate(2, constructor_conflict,
               'pin behavior when one key is constructed with non-key fields that disagree with the current target row').
verification(key_edge_case_census, passed(12)).

touched('v6/prolog/0_program_check.pl',
        'shared key-position range and duplicate invariants').
touched('v6/prolog/conformance/engine.pl',
        'oracle refusal order and vocabulary for invalid key positions').
touched('v6/prolog/compile/analyze.pl',
        'compiler refusal order and vocabulary matching the oracle').
touched('v6/prolog/compile/test/plunit_tests.pl',
        'three compiler refusals and two cross-door parity receipts').

closed(key_position_validation_hole,
       'zero and above-arity positions now refuse as key_position_out_of_range; repeated positions refuse as key_position_duplicate before lowering').
observed(key_validation_cost,
         'two shared program invariants, two refusal mappings per door, and five unit receipts; no parser, SQL, runtime, or surface change').
verification(key_validation_plunit, passed(147)).
verification(key_validation_conformance, passed(163)).
verification(key_validation_census, passed(12)).
next_action(1,
            'pin keyed replacement and exact-row retraction against current tick logs, including stale old-row removal after replacement').

touched('v6/prolog/conformance/fixtures/scopes.pl',
        'paired oracle and emitted-SQL receipt for delayed old-row retraction after keyed replacement').
locked(keyed_signed_row_clock,
       'positive arrival replaces the row at its key; negative arrival retracts the exact row named; delayed -old is silent after +new; -current removes current').
observed(keyed_clock_cost,
         'four scheduled ticks: +v1; replacement -v1/+v2; delayed -v1 produces no delta; -v2 removes current').
observed(key_delete_surface,
         'the language has signed full rows and no distinct key-delete operation; exact negative rows preserve occurrence identity without another primitive').
verification(stale_keyed_retraction_oracle, passed(3)).
verification(stale_keyed_retraction_sql, passed(3)).
next_action(1,
            'grade reference construction contexts: existing target query, missing target, nested world arrival, and same-key conflicting non-key fields').

touched('v6/prolog/labs/rel_value_unification/9_reference_construction_contexts.pl',
        'seven real compiler checks across existing, missing, conflicting, key-only, runtime, and boot reference construction').
observed(reference_construction_contexts,
         '7 checks pass against current plan/lowering/runtime SQL generation').
observed(existing_target_id_discarded,
         'an RHS target relation already joins the target table, but the nested head constructor discards that available row identity into JSON').
observed(missing_target_has_no_join,
         'a nested constructor without an RHS target atom currently fabricates JSON without checking target membership').
observed(conflicting_non_key_runtime_failure,
         'runtime INSERT OR IGNORE respects the key UNIQUE constraint, then full-row lookup by key plus non-key fields returns no id for a same-key conflict').
observed(boot_schema_divergence,
         'boot reference insertion still queries removed __semantic columns absent from current target-table DDL').
leading_hypothesis(derived_reference_lowering,
                   'relation-shaped head value is an indexed match against an existing public target row; its fields constrain that row and the parent stores __id').
leading_hypothesis(missing_derived_target,
                   'ordinary join semantics produce no parent row; target creation is an ordinary target-headed rule participating in the relational fixpoint').
leading_hypothesis(world_reference_arrival,
                   'boundary batch resolves or asserts target rows before parent rows atomically; conflicts refuse the batch by name').
leading_hypothesis(key_only_constructor,
                   'no key-only arity is needed while a full relation pattern with existing wildcards can constrain only key fields').
verification(reference_construction_contexts, passed(7)).
next_action(1,
            'prototype existing-target relation constructor as a direct __id projection from the already joined target table, with no JSON and no hidden write').

touched('v6/prolog/compile/lower.pl',
        'prototype binds each positive body atom for a referenced public relation to its table alias __id and lets the identical relation-shaped head term project it').
observed(existing_target_identity_prototype,
         'post(user(Id,Name)) from a body containing user(Id,Name) now selects bN.__id directly; no JSON, subquery, extra statement, hidden target write, or surface syntax').
observed(existing_target_identity_cost,
         'one compile-time compound-term binding per referenced positive use and one early bound-expression lookup').
observed(direct_trigger_gap,
         'a referenced relation used as the direct edge trigger still binds only its fields and emits JSON for a relation-shaped head value').
observed(implicit_match_gap,
         'a relation-shaped head value without an explicit target atom still emits JSON; automatic RHS match injection is unimplemented').
verification(existing_target_identity_context_lab, passed(8)).
verification(existing_target_identity_plunit, passed(147)).
verification(existing_target_identity_sweep_compile,
             result(164,103,61,0,
                    'total, compiled, unsupported, crash')).
verification(existing_target_identity_sweep_runtime,
             result(103,92,9,2,
                    'compiled, identical, expected old relation-value oracle disagreements, recorded run errors')).
next_action(1,
            'extend the same identity binding to direct triggers, then test automatic target-match injection separately from world-arrival assertion').

observed(direct_trigger_identity_prototype,
         'an arrival trigger that is itself a referenced public relation now samples that current row by its trigger fields and projects the joined __id').
observed(direct_trigger_identity_cost,
         'one indexed equality join to the target table in edge projection; departure triggers retain occurrence-only semantics and do not join absent membership').
verification(direct_trigger_identity_context_lab, passed(8)).
verification(direct_trigger_identity_plunit, passed(147)).
next_action(1,
            'lab automatic target-match injection for a relation-shaped head term lacking an explicit target atom, including recursive level fixpoint timing').

% Sol clock/type/calculus shakedown requested alongside the relation-edge
% fixpoint.
touched('plans/2026-07-29-clock-checker-proof-payoff.md',
        'actual-world audit of implemented DL6 static proofs, runtime receipts, and proof gaps against Rust, Lustre, and Esterel').
touched('v6/prolog/ARCH.pl',
        'clock_checker_proof_payoff task row with ranked zero-surface checker iteration').
observed(clock_checker_static_proofs,
         'current compiler statically proves negative and aggregate stratification, rejects positive recursive SQL strata, rejects five named cross-plane placements, and validates declaration/key shape').
observed(clock_checker_runtime_only,
         'general tick placement, arrival/level seam behavior, keyed batch order, provider response timing, and oracle/emitter equality are fixture or runtime properties rather than inferred program-wide theorems').
observed(rust_comparison,
         'relation references are durable integer graph edges rather than borrowed memory locations; the applicable lifetime property is live parent endpoint implies live target membership at a transaction boundary, expressible as an antijoin').
observed(lustre_comparison,
         'DL6 rule-graph grade corresponds to synchronous delay while B/N/Z ring composition adds relational cardinality; current named refusals are hand-coded instances of the combined calculus').
observed(esterel_comparison,
         'monotone B least fixpoint plus stratified absence supplies restricted constructive same-tick semantics; labelled zero-grade SCC analysis is absent for occurrence, sign, sampling, and effect dependencies').
leading_hypothesis(clock_checker_fixpoint,
                   'derive labelled dependency facts from existing AST and registry, infer clock offsets, then check zero-grade SCC constructiveness before adding any language spelling').
leading_hypothesis(reference_lifetime_check,
                   'compile live-parent and absent-target into an incremental boundary antijoin; defer cascade, retention, ownership, and per-edge policy until an actual lifecycle receipt requires one').
verification(clock_checker_plan_options,
             passed('four decision cards, each containing no more than five options')).
next_action(clock_checker_1,
            'select D1 and add internal ring/sign/grade metadata with no parser or surface change').
next_action(clock_checker_2,
            'derive tick offsets for existing edge-chain and pipe fixtures and compare them with observed tick logs').
next_action(clock_checker_3,
            'grade monotone, negative, occurrence-sensitive, and positive-delay SCC counterexamples through the real parser and analyzer').

touched('v6/prolog/0_relation_edge_expand.pl',
        'shared post-match expansion from relation-shaped head values to visible target membership dependencies').
touched('v6/prolog/1_expansion.pl',
        'phase 50 relation_edge after enum, spread placeholders, and match').
touched('v6/prolog/labs/rel_value_unification/10_reference_fixpoint_clock.pl',
        'six oracle/compiler receipts for same-tick target creation, missing targets, and keyed replacement').
locked(automatic_derived_reference_match,
       'a relation-shaped value in a level head adds an ordinary target atom; an edge head adds latest(target) so membership is sampled without adding a trigger').
locked(derived_missing_target,
       'missing target membership yields no parent derivation through ordinary join semantics').
locked(derived_reference_clock,
       'an edge-written keyed target and a level-derived parent settle in the same tick; replacing the target retracts old parent and adds new parent in that tick').
observed(automatic_match_checker_visibility,
         'the generated target dependency enters shared expansion before program checks, stratification, SQL planning, oracle evaluation, and emitted lowering').
observed(automatic_match_feature_cost,
         'one shared expansion phase and no surface syntax or runtime primitive').
verification(reference_edge_expansion_plunit, passed(150)).
verification(reference_edge_expansion_conformance, passed(164)).
verification(reference_fixpoint_clock_lab, passed(6)).
verification(reference_construction_context_lab, passed(8)).
next_action(1,
            'replace boot/world full-row StructPlane semantics with key-driven batched target resolution and named conflicts').

touched('v6/prolog/compile/lower.pl',
        'key-driven batched target plans, computed target render views, and key-based boot target resolution').
touched('v6/tsv2/runtime/structPlane.ts',
        'three-statement target resolution: conflict preflight, set insert, key lookup').
touched('v6/tsv2/tests/structPlane.test.ts',
        'ten runtime receipts including flat statement count and three key-conflict cases').
closed(boot_semantic_blob_path,
       'boot recursively inserts target relation rows and resolves parent endpoints by declared key or full-row fallback; generated SQL contains no __semantic or __rendered storage column').
locked(world_reference_conflict,
       'same key plus equal full row reuses one id; same key plus different non-key fields refuses relation_reference_conflict before parent rewriting; a conflicting same-batch pair executes zero SQL statements').
locked(reference_boundary_render,
       'relation rows store only typed columns and __id; __ref_<rel> is a temporary computed view that reconstructs boundary JSON without storing JSON').
observed(reference_resolution_cost,
         'three set-based SQL statements per referenced target relation with values in a tick, flat from 3 through 50 requested rows').
observed(reference_target_visibility_diff,
         'nine former struct fixtures now expose resolver-created target rows in final state because referenced declarations are public rels; old oracle final state suppresses those rows').
observed(reference_tick_visibility,
         'resolver-created target rows are immediately queryable current membership but are not staged as outside-arrival deltas; corpus tick logs remain byte-identical').
observed(reference_transaction_constraint,
         'wrapping target resolution and the whole emitted tick in SqlRunner.inTransaction fails because incremental tick paths call executeMultiple, whose driver rollback guard closes the transaction').
verification(world_reference_runtime_receipts, passed(10)).
verification(world_reference_construction_lab, passed(8)).
verification(world_reference_plunit, passed(150)).
verification(world_reference_typecheck, passed).
verification(world_reference_sweep,
             result(103,101,0,2,
                    'compiled, identical, wrong, recorded run errors')).
verification(world_reference_final_state,
             result(103,92,9,2,
                    'compiled, identical, target-visibility diffs, recorded run errors')).
next_action(1,
            'pin whether resolver-created target membership crosses the delta boundary or remains silent identity materialization, then update the oracle final-state contract').
next_action(2,
            'split identity dependency cycles from non-key relation-edge cycles and grade self and mutual reference cases').
next_action(3,
            'run the opaque identity transport cases; retain ref only if a current program cannot express the required edge through relation matching and modes').
