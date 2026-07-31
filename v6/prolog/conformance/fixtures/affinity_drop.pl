% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Fail-first fixture for the incremental emitter's post-affinity arrival
% delta. The source and head columns have no declared type.
fixture(arrival_affinity_rewrite_keeps_delta,
  prog([ kind(probe_in/1, log), keep(probe_in/1, all) ],
       [ (probe_out(ProbeValue) <- probe_in(ProbeValue)) ]),
  [],
  [ [ +probe_in(4) ] ],
  [ deltas(probe_in/1, [ [ +probe_in(4) ] ]),
    deltas(probe_out/1, [ [ +probe_out(4) ] ]),
    final(probe_out/1, [ probe_out(4) ]) ]).

% Coordinator adversarial companion: the fix pairs entries with RETURNING rows
% POSITIONALLY, and INSERT OR IGNORE returns FEWER rows than a batch holding
% duplicates. This interleaved dup/new/dup/new batch proves the emitted delta
% set still equals the RETURNING set (only internal sequence attribution
% skews, which the graded log never observes). Hand-graded byte-identical on
% both doors before promotion (2026-07-30 coordinator runs).
fixture(arrival_dup_batch_partial_ignore,
  prog([ col_type(seen/1, value, text) ],
       [ (derived(SeenValue) <- seen(SeenValue)) ]),
  [],
  [ [ +seen(alpha), +seen(gamma) ],
    [ +seen(alpha), +seen(beta), +seen(gamma), +seen(delta) ] ],
  [ deltas(seen/1, [ [ +seen(alpha), +seen(gamma) ],
                     [ +seen(beta), +seen(delta) ] ]),
    deltas(derived/1, [ [ +derived(alpha), +derived(gamma) ],
                        [ +derived(beta), +derived(delta) ] ]),
    final(derived/1, [ derived(alpha), derived(beta), derived(delta), derived(gamma) ]) ]).
