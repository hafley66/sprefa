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
