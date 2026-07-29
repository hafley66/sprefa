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
