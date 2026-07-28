% fixtures/operators.pl : the small-operator jutsu receipts. filter and map
% are level rules; repeat is a self-carry chain the engine's own drains run;
% forkJoin is a conjunctive body (wait-for-all = the join derives when the
% last input lands). Owner: coordinator.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% filter + map in one rule: guard goal filters, head expression maps. The
% view RETRACTS when the source row leaves (filter over a live set).
fixture(filter_map_is_a_level_rule,
  prog([ kind(reading/2, set) ],
       [ (doubled(Name, Out) <- reading(Name, Value), Value >= 10, Out := Value * 2) ]),
  [ reading(cpu, 12), reading(disk, 4) ],
  [ [ +reading(net, 30) ],
    [ -reading(cpu, 12) ] ],
  [ deltas(doubled/2, [ [ +doubled(net, 60) ], [ -doubled(cpu, 24) ] ]),
    final(doubled/2, [ doubled(net, 60) ]) ]).

% repeat(3): a self-carry chain with a guard. One outside kick, the engine's
% drain scheduler runs the remaining iterations, the guard stops the carry
% (dedalus stop-carrying as loop exit).
fixture(repeat_is_a_self_carry_chain,
  prog([ kind(kick/1, log),  keep(kick/1, all),
         kind(pulse/1, log), keep(pulse/1, all) ],
       [ (pulse(1) <+ kick(_)),
         (pulse(Next) <+ pulse(SoFar), SoFar < 3, Next := SoFar + 1) ]),
  [],
  [ [ +kick(go) ] ],
  [ deltas(pulse/1, [ [ +pulse(1) ], [ +pulse(2) ], [ +pulse(3) ], [] ]),
    ticks(4) ]).

% forkJoin: combined output exists only once EVERY input does; the empty
% middle tick grades the silence while one arm is still outstanding.
fixture(fork_join_is_a_conjunctive_body,
  prog([ kind(result_a/1, set), kind(result_b/1, set) ],
       [ (combined(ValueA, ValueB) <- result_a(ValueA), result_b(ValueB)) ]),
  [],
  [ [ +result_a(alpha) ],
    [],
    [ +result_b(beta) ] ],
  [ deltas(combined/2, [ [], [], [ +combined(alpha, beta) ] ]),
    final(combined/2, [ combined(alpha, beta) ]) ]).

% forkJoin over ENVELOPES: failure is a value, so "any input errored" is a
% second join, not an exception path; the ok join stays silent forever.
fixture(fork_join_error_arm_is_a_value,
  prog([ kind(outcome_a/1, set), kind(outcome_b/1, set) ],
       [ (both_ok(BodyA, BodyB) <- outcome_a(ok(BodyA)), outcome_b(ok(BodyB))),
         (any_failed(Status) <- outcome_a(error(Status))),
         (any_failed(Status) <- outcome_b(error(Status))) ]),
  [],
  [ [ +outcome_a(ok(body_one)) ],
    [ +outcome_b(error(502)) ] ],
  [ final(both_ok/2, []),
    final(any_failed/1, [ any_failed(502) ]) ]).
