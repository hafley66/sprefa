% V6 completion drive: ordered state, ghcacher golden, extract golden.
%
% Context
% -------
% Base: 0e79d6ec.
% The higher-order scan lab proved switchMap lowers through the existing
% compiler and SQLite. scan and switchScan share one blocker:
% edge_body_needs_pre, already owned by pre_occurrence_loop across 13 fixtures.
% Ghcacher and extraction-live already exist; this drive turns their timelines
% into the two standing clock goldens.
%
% Decisions
% ---------
% One time model: Store[T] x Events[T] -> Store[T+1].
% Pure closure and ordered reducer steps run inside T.
% Committed boundary deltas feed listeners; listener results enter a later
% event queue. No runtime function values, dynamic relation tables, or second
% switch runtime. Higher-order calls specialize before checked IR and SQLite.
%
% Verification
% ------------
% Every lane starts narrow. The coordinator alone runs the final full battery
% and regenerates shared manifests. Existing unrelated dirty flow fixtures,
% manifest drift, and reference-callback prototype files are excluded.
%
% Staffing
% --------
% lane ordered_event_scan: Sol, production compiler/runtime, no commit.
% lane ghcacher_tick_golden: Sol, fixtures/lab/script only, no commit.
% lane extract_tick_golden: Terra, fixtures/lab/script only, no commit.
% Coordinator owns ARCH, this plan, session log, shared generated files,
% integration commits, and push.

:- module(v6_completion_drive,
          [ drive_goal/2,
            drive_task/5,
            exit_receipt/2,
            forbidden/2,
            next_ready/1
          ]).

drive_goal(clock_model,
           'one transaction tick with pure closure, ordered keyed scan, one net boundary delta, and listener feedback on a later event queue').
drive_goal(v5_usurp,
           'compile the 13 pre fixtures, then hold ghcacher and extraction as standing end-to-end clock goldens').
drive_goal(reversible_higher_order,
           'named rule arguments specialize to ordinary rules; syntax waits until repeated golden use proves the spelling').

% drive_task(Name, Status, Dependencies, Owner, Deliverable).
drive_task(ordered_event_scan, active, [],
           sol,
           'emitted runtime processes queued events in order, exposes each keyed write to the next reducer step, and publishes one net boundary delta').
drive_task(ghcacher_tick_golden, active, [],
           sol,
           'one deterministic schedule proves demand, committed listener activation, host response, keyed replacement, late-result rejection, and final state').
drive_task(extract_tick_golden, active, [],
           terra,
           'one deterministic file edit proves extract demand, multi-row response, derived findings, retraction, and bounded statement shape').
drive_task(clock_signature_projection, queued,
           [ordered_event_scan],
           coordinator,
           'existing graph projects type, key, plane, grade, cardinality, lifetime, reads, writes, and effects as queryable proof facts').
drive_task(switch_map_expansion, queued,
           [ordered_event_scan, ghcacher_tick_golden],
           coordinator,
           'checker-visible switchMap sugar expands to keyed scope, demand, and view rules with no runtime switch operation').
drive_task(scan_specialization, queued,
           [ordered_event_scan, extract_tick_golden],
           coordinator,
           'named reducer rule specializes to keyed state rules and the explicit canonical signature used by later inference').
drive_task(golden_gate, queued,
           [ghcacher_tick_golden, extract_tick_golden],
           coordinator,
           'both goldens run from one just target and fail on tick-log, final-state, or statement-count drift').

exit_receipt(ordered_event_scan,
             '13 edge_body_needs_pre fixtures leave unsupported; oracle and naive/incremental emitted tick logs match; intermediate reducer states stay boundary-invisible').
exit_receipt(ghcacher_tick_golden,
             'hermetic host schedule, exact tick log, exact final rels, stale response rejected by current scope membership').
exit_receipt(extract_tick_golden,
             'hermetic release extractor fixture, exact multi-row deltas, edit removes old findings, statement count independent of response row count').
exit_receipt(final,
             'conformance, plunit, text-door, roundtrip, both sweep modes, tsv2 tests, ghcacher golden, and extract golden pass').

forbidden(all_lanes, 'new surface spelling').
forbidden(all_lanes, 'runtime function-valued relation columns').
forbidden(all_lanes, 'dynamic relation creation').
forbidden(all_lanes, 'editing unrelated dirty files').
forbidden(golden_lanes, 'production compiler or runtime edits').
forbidden(agent_lanes, 'commits, pushes, or shared manifest regeneration').

next_ready(Name) :-
    drive_task(Name, queued, Dependencies, _, _),
    forall(member(Dependency, Dependencies),
           drive_task(Dependency, done, _, _, _)).
