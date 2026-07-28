% collapse.pl : THREAD 4, the ruled transition-collapse trace event made
% concrete.
%
% rulings.pl transition_rule_semantics: when a transition-consuming rule's
% boundary collapses multiple replaces, the runtime logs the collapse through
% the tracing spine, naming rel, key and the number of collapsed occurrences,
% never silently.
%
% This file answers four questions the ruling leaves open:
%   1. WHERE does the event fire (how many instrumentation points)
%   2. WHAT is the count
%   3. does it fire when the boundary shows NOTHING
%   4. is the event graded or ungraded

:- module(ca_collapse, [ collapse_scenario/2, collapse_site/2, collapse_slot/2 ]).

:- use_module(library(lists)).
:- use_module(oracle).
:- use_module(model).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous collapse_scenario/2.

% ═══ the event ═════════════════════════════════════════════════════════════
%
% collapse(Tick, Ref, Key, Writes, NetVisible)
%
% .dl surface: none. The event is a runtime trace line, not a rel, exactly
% the way rulings.pl effect_abort puts the abort warning at the abort site.
%
% rx lowering of the thing being instrumented:
%   source$.pipe(
%     groupBy(row => row.key),
%     mergeMap(group => group.pipe(
%       bufferWhen(() => boundary$),
%       tap(batch => { if (batch.length > 1) traceCollapse(ref, group.key, batch.length); }),
%       map(batch => batch[batch.length - 1]))))
%   -- bufferWhen(boundary$) IS the tick; the buffer length IS the count; and
%   taking the last element IS the collapse. The trace line is a tap on the
%   buffer, which is why one instrumentation point suffices.

% ═══ instrumentation sites ═════════════════════════════════════════════════
% collapse_site(Site, Reachable). Every frontier the ruling names funnels
% through the keyed store write in this engine, so there is exactly ONE site.

collapse_site(keyed_store_write, yes).
collapse_site(set_rel_duplicate_add, no).
collapse_site(log_rel_append, no).
collapse_site(level_rel_recompute, no).

collapse_scenario(r1_exactly_one_instrumentation_site_is_reachable, Goal) :-
    Goal = ( findall(Site, collapse_site(Site, yes), Sites),
             Sites == [keyed_store_write] ).

% A duplicate Set add is not a collapse: no value was lost, and engine.pl
% does not even mint an occurrence for it (:192-195).
collapse_scenario(r1_a_duplicate_set_add_is_not_a_collapse, Goal) :-
    Goal = ( crun(cprog([], [ crule(arr(src(Value)), [], out(Value)) ]),
                  [], [[ +src(a), +src(a) ]], 100, Log, Collapses),
             Collapses == [],
             Log == [ line(1, [ +out(a), +src(a) ]), line(2, []) ] ).

% A Log append is not a collapse: every stamp is distinct and every row
% survives.
collapse_scenario(r1_a_log_append_is_not_a_collapse, Goal) :-
    Goal = ( crun(cprog([ kind(event/1, log) ], []),
                  [], [[ +event(a), +event(a) ]], 100, _, Collapses),
             Collapses == [] ).

% ═══ the model is cross-checked against the oracle BEFORE it is trusted ════

collapse_prog(prog([ kind(poll/2, log), keep(poll/2, all), keyed(latest/2, [1]) ],
                   [ (latest(Key, Value) <+ poll(Key, Value)) ])).

model_collapse_prog(cprog([ kind(poll/2, log), keyed(latest/2, [1]) ],
                          [ crule(arr(poll(Key, Value)), [], latest(Key, Value)) ])).

collapse_scenario(r1_model_and_oracle_agree_on_the_collapse_program, Goal) :-
    collapse_prog(OracleProg), model_collapse_prog(ModelProg),
    Goal = ( oracle_log(OracleProg, [latest(cli, v0)],
                        [[ +poll(cli, v1), +poll(cli, v2) ]], OracleLog),
             crun(ModelProg, [latest(cli, v0)],
                  [[ +poll(cli, v1), +poll(cli, v2) ]], 100, ModelLog, _),
             findall(Deltas, member(line(_, Deltas), ModelLog), ModelDeltas),
             OracleLog == ModelDeltas ).

% ═══ ROUND 1 : the event, with a real count ════════════════════════════════
% Two writes to one key in one tick. The oracle shows one delta pair; the
% model additionally names the collapse.

collapse_scenario(r1_two_writes_one_key_one_tick_mints_one_event, Goal) :-
    model_collapse_prog(Prog),
    Goal = ( crun(Prog, [latest(cli, v0)], [[ +poll(cli, v1), +poll(cli, v2) ]],
                  100, _, Collapses),
             Collapses == [ collapse(1, latest/2, [cli], 2, true) ] ).

% One write each to two keys is not a collapse at all.
collapse_scenario(r1_one_write_per_key_mints_no_event, Goal) :-
    model_collapse_prog(Prog),
    Goal = ( crun(Prog, [latest(cli, v0), latest(api, w0)],
                  [[ +poll(cli, v1), +poll(api, w1) ]], 100, _, Collapses),
             Collapses == [] ).

% Three writes to one key mint ONE event with count 3, not two events. The
% count is WRITES, not invisible intermediates: a reader of the trace line
% wants to know how many occurrences the boundary saw, and can subtract one
% themselves.
collapse_scenario(r1_three_writes_mint_one_event_counting_writes_not_intermediates, Goal) :-
    model_collapse_prog(Prog),
    Goal = ( crun(Prog, [latest(cli, v0)],
                  [[ +poll(cli, v1), +poll(cli, v2), +poll(cli, v3) ]],
                  100, _, Collapses),
             Collapses == [ collapse(1, latest/2, [cli], 3, true) ] ).

% ═══ ROUND 2 : the net-zero collapse ═══════════════════════════════════════
% This breaks the round-1 reading "the event annotates a delta". Write v1
% then write v0 back and the boundary shows NOTHING for latest/2 at all. The
% oracle receipt first: one tick, no latest delta.

collapse_scenario(r2_a_net_zero_pair_of_writes_shows_no_delta_at_all, Goal) :-
    collapse_prog(Prog),
    Goal = ( oracle_log_final(Prog, [latest(cli, v0)],
                              [[ +poll(cli, v1), +poll(cli, v0) ]], Final, Log),
             Log == [ [ +poll(cli, v1), +poll(cli, v0) ] ],
             final_has(Final, latest(cli, v0)) ).

% So the event MUST fire on write count, not on delta presence, or the one
% case where silence is most misleading is the one case that stays silent.
collapse_scenario(r2_the_event_still_fires_on_the_net_zero_pair, Goal) :-
    model_collapse_prog(Prog),
    Goal = ( crun(Prog, [latest(cli, v0)], [[ +poll(cli, v1), +poll(cli, v0) ]],
                  100, Log, Collapses),
             Collapses == [ collapse(1, latest/2, [cli], 2, false) ],
             Log == [ line(1, [ +poll(cli, v1), +poll(cli, v0) ]) ] ).

% An equal-row rewrite is a no-op at the store (r_equal_row_write) but it IS
% an occurrence that the boundary swallowed, so it counts. Writing v1 twice
% collapses two occurrences into one delta.
collapse_scenario(r2_an_equal_row_rewrite_still_counts_as_a_collapsed_write, Goal) :-
    model_collapse_prog(Prog),
    Goal = ( crun(Prog, [latest(cli, v0)], [[ +poll(cli, v1), +poll(cli, v1) ]],
                  100, _, Collapses),
             Collapses == [ collapse(1, latest/2, [cli], 2, true) ] ).

% ═══ ROUND 3 : the event is not gradeable where the ruling puts it ═════════
% The ruling puts the event on the tracing spine, which is not the tick log.
% Item 9 of the stopping point grades runners by diffing tick logs. So two
% runners can disagree about collapse counts and both grade PASS: the model
% run below has a nonempty collapse list and a tick log that is byte-identical
% to a run with an empty one.

collapse_scenario(r3_two_runs_with_different_collapse_counts_share_a_tick_log, Goal) :-
    model_collapse_prog(Prog),
    Goal = ( crun(Prog, [latest(cli, v0)], [[ +poll(cli, v2) ]], 100, LogOne, One),
             crun(Prog, [latest(cli, v0)], [[ +poll(cli, v1), +poll(cli, v2) ]],
                  100, LogTwo, Two),
             One == [],
             Two == [ collapse(1, latest/2, [cli], 2, true) ],
             tick_log_of_ref(LogOne, latest/2, Shape),
             tick_log_of_ref(LogTwo, latest/2, Shape) ).

tick_log_of_ref(Log, Name/Arity, Shape) :-
    findall(Kept,
            ( member(line(_, Deltas), Log),
              findall(Delta,
                      ( member(Delta, Deltas),
                        ( Delta = +Row ; Delta = -Row ), functor(Row, Name, Arity) ),
                      Kept) ),
            Shape).

collapse_slot('SLOT-COLLAPSE-CHANNEL',
              'the collapse event is required to be observable and is placed on the tracing spine, which the item-9 tick-log grading does not read. Either the grading harness diffs a second collapse log alongside the tick log, or the event moves into the tick log as a distinguished line. A trace-only event cannot be conformance-checked at all').

collapse_scenario(r3_the_grading_gap_is_a_named_slot, Goal) :-
    Goal = ( collapse_slot('SLOT-COLLAPSE-CHANNEL', Why),
             sub_atom(Why, _, _, _, 'tick log') ).
