% fixtures/3_flagship_callgraph.pl: the DERIVATION CORE of the alpha's flagship
% rail, graded against the oracle on a canned extraction schedule.
%
% v6/dl/fixtures/flagship-callgraph.dl6 is the whole program: two `sh` hosts over
% `sprefa-extract`, a `git ls-files` file feed, and three derived rels. The hosts
% cannot ride a fixture (nothing spawns here), and they do not need to: a host
% answer IS an ordinary EDB arrival, so the schedule below feeds `node_fact` and
% `call` rows exactly as the served runtime commits them and the three RULES
% under test are the program's own, character for character.
%
% They are v5's `examples/callgraph-ast.dl` rules, which is the point:
%
%     calls(caller, callee) <-
%       def(caller, p, _), call(callee, p, _), def(callee, _, _), caller != callee.
%     unused(name) <- def(name, _, _), !call(name, _, _).
%
% `v6/tsv2/scripts/flagship-callgraph.sh` grades the live end-to-end run against
% v5's own output on a pinned corpus; these two fixtures grade the derivation
% against the oracle, both emitter modes, which is the half a live diff cannot
% see (a tick log, not an end state).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ callgraph_derivation_over_extraction: the join, tick by tick ═══════════
%
% `node_fact` is the extractor's `record=node` line shape, tag included. The tag
% is not decoration: `--family call` interleaves `record=specifier` lines that
% carry a `name` and a `kind` exactly like a node line does, so a projection on
% (kind, name) alone would silently mint defs out of import bindings. The
% specifier row in tick 1 is the decoy that proves the constant match filters it.
%
% Tick by tick:
%   1  two defs in a.rs plus the specifier decoy. Nothing is called yet, so both
%      defs are `unused` and `calls` is empty.
%   2  a.rs calls `helper` (a def) and `missing` (not a def anywhere). One
%      `calls` row: `main -> helper`. `helper -> helper` is cut by the
%      caller \== callee guard and `missing` by the def-membership atom. `helper`
%      leaves `unused` -- the negated rel RETRACTS, which is the delta this
%      fixture exists to pin.
%   3  a SECOND file defines `helper` and calls `main`. The cross-file
%      def-membership atom is what lets b.rs's caller reach a.rs's def, and the
%      graph closes into a 2-cycle. `unused` empties.
%   4  the b.rs call is retracted. `calls` loses its row and `main` comes BACK
%      into `unused`: the negated rel re-asserts through the retraction path,
%      which is the leg an insert-only schedule never touches.
fixture(callgraph_derivation_over_extraction,
  prog([ col_type(node_fact/4, path, text),
         col_type(node_fact/4, record, text),
         col_type(node_fact/4, kind, text),
         col_type(node_fact/4, name, text),
         col_type(call/2, path, text),
         col_type(call/2, callee, text),
         col_type(def/3, path, text),
         col_type(def/3, name, text),
         col_type(def/3, kind, text),
         col_type(calls/2, caller, text),
         col_type(calls/2, callee, text) ],
       [ (def(Path, Name, Kind) <- node_fact(Path, node, Kind, Name)),
         (calls(Caller, Callee) <-
            (def(Path, Caller, _),
             call(Path, Callee),
             def(_, Callee, _),
             Caller \== Callee)) ]),
  [],
  [ [ +node_fact('a.rs', node, function, main),
      +node_fact('a.rs', node, method, helper),
      +node_fact('a.rs', specifier, named, helper) ],
    [ +call('a.rs', helper),
      +call('a.rs', missing) ],
    [ +node_fact('b.rs', node, function, helper),
      +call('b.rs', main) ],
    [ -call('b.rs', main) ] ],
  [ deltas(def/3, [ [ +def('a.rs', helper, method), +def('a.rs', main, function) ],
                    [],
                    [ +def('b.rs', helper, function) ],
                    [] ]),
    deltas(calls/2, [ [],
                      [ +calls(main, helper) ],
                      [ +calls(helper, main) ],
                      [ -calls(helper, main) ] ]),
    final(def/3, [ def('a.rs', helper, method),
                   def('a.rs', main, function),
                   def('b.rs', helper, function) ]),
    final(calls/2, [ calls(main, helper) ]) ]).

% ═══ callgraph_unused_inverts_with_the_call_set ════════════════════════════
%
% The same schedule, graded on the NEGATED rel alone, because it is the one that
% moves the other way. `unused(Name) <- def(_, Name, _), not(call(_, Name))` is
% anti-monotone in `call`: growing the call set SHRINKS unused. That is why the
% live rig's table shows v6 a strict superset of v5 on def/call/calls and a
% strict SUBSET on unused, and why the flagship report reads that inversion as
% the semantics working rather than as a defect.
%
% Both directions are graded here: tick 2 and tick 3 retract a row from `unused`
% as calls arrive, tick 4 re-asserts one as a call is retracted, tick 5 retracts
% it again as a different call arrives. `helper` never comes back, because a.rs
% still calls it.
%
% WHY TICK 5 EXISTS, stated because it is not extra coverage for its own sake.
% Ending the schedule at tick 4 makes this fixture WRONG in the sweep, and the
% divergence is the EMITTER's, not this fixture's:
%
%   oracle  4 ticks
%   tsv2    5 ticks, the fifth {"tick":5,"deltas":{}}
%
% A row RE-ASSERTED by the refCount reseed on a retraction tick stages a
% next-frontier row, `promoteFrontiers` reports `carryPending`, and `TickFold`
% mints one empty drain tick the oracle never has. It needs a non-monotone level
% rule to be observable at all, which is why 137 fixtures never hit it: on this
% same schedule the sibling fixture above (whose last tick only DELETES a level
% row) is byte-identical, and truncating THIS fixture to three ticks is also
% byte-identical. Ending on a tick whose level delta is negative is what keeps
% the fixture on the graded side of that boundary. The gap belongs to the
% arc that owns runtime/1_incremental.ts; the repro is the four-tick prefix of
% the schedule below.
fixture(callgraph_unused_inverts_with_the_call_set,
  prog([ col_type(node_fact/4, path, text),
         col_type(node_fact/4, record, text),
         col_type(node_fact/4, kind, text),
         col_type(node_fact/4, name, text),
         col_type(call/2, path, text),
         col_type(call/2, callee, text),
         col_type(def/3, path, text),
         col_type(def/3, name, text),
         col_type(def/3, kind, text),
         col_type(unused/1, name, text) ],
       [ (def(Path, Name, Kind) <- node_fact(Path, node, Kind, Name)),
         (unused(Name) <- (def(_, Name, _), not(call(_, Name)))) ]),
  [],
  [ [ +node_fact('a.rs', node, function, main),
      +node_fact('a.rs', node, method, helper),
      +node_fact('a.rs', specifier, named, helper) ],
    [ +call('a.rs', helper),
      +call('a.rs', missing) ],
    [ +node_fact('b.rs', node, function, helper),
      +call('b.rs', main) ],
    [ -call('b.rs', main) ],
    [ +call('a.rs', main) ] ],
  [ deltas(unused/1, [ [ +unused(helper), +unused(main) ],
                       [ -unused(helper) ],
                       [ -unused(main) ],
                       [ +unused(main) ],
                       [ -unused(main) ] ]),
    final(unused/1, []) ]).
