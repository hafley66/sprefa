% fixtures/4_flagship_flow.pl: recursive callable-plane closure used by the
% portable v6 leg of examples/flow-interproc.dl.
%
% The live rail receives resolved_call_edge rows from `extract --resolve`.
% This fixture supplies those rows directly and grades the two-rule closure:
% base edges appear, then each composable resolved edge extends the reach set.
% The fixture deliberately has no df rows. The v6 extractor has no df family,
% and the live program must leave the value-plane relations unmanufactured.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

fixture(flagship_flow_reach_over_resolved_edges,
  prog([ col_type(resolved_call_edge/4, caller_path, text),
         col_type(resolved_call_edge/4, caller_name, text),
         col_type(resolved_call_edge/4, callee_path, text),
         col_type(resolved_call_edge/4, callee_name, text),
         col_type(flow_edge/4, from_path, text),
         col_type(flow_edge/4, from_name, text),
         col_type(flow_edge/4, to_path, text),
         col_type(flow_edge/4, to_name, text),
         col_type(flow_reach/4, from_path, text),
         col_type(flow_reach/4, from_name, text),
         col_type(flow_reach/4, to_path, text),
         col_type(flow_reach/4, to_name, text) ],
       [ (flow_edge(FromPath, FromName, ToPath, ToName) <-
            resolved_call_edge(FromPath, FromName, ToPath, ToName)),
         (flow_reach(FromPath, FromName, ToPath, ToName) <-
            flow_edge(FromPath, FromName, ToPath, ToName)),
         (flow_reach(FromPath, FromName, ToPath, ToName) <-
            (flow_reach(FromPath, FromName, MiddlePath, MiddleName),
             flow_edge(MiddlePath, MiddleName, ToPath, ToName))) ]),
  [],
  [ [ +resolved_call_edge(app, entry, lib, decode) ],
    [ +resolved_call_edge(lib, decode, sink, persist) ] ],
  [ final(flow_edge/4, [ flow_edge(app, entry, lib, decode),
                         flow_edge(lib, decode, sink, persist) ]),
    final(flow_reach/4, [ flow_reach(app, entry, lib, decode),
                          flow_reach(app, entry, sink, persist),
                          flow_reach(lib, decode, sink, persist) ]) ]).

% SABOTAGE RECEIPT: same two rules, both hops in ONE tick. With
% applyLevelsBeforeEdges running a single round, flow_reach(app,entry,sink,persist)
% is never derived and the tick log loses the row. Every other recursive fixture
% feeds one hop per tick, so the one-round-per-tick defect passes all of them.
fixture(flagship_flow_reach_over_batched_resolved_edges,
  prog([ col_type(resolved_call_edge/4, caller_path, text),
         col_type(resolved_call_edge/4, caller_name, text),
         col_type(resolved_call_edge/4, callee_path, text),
         col_type(resolved_call_edge/4, callee_name, text),
         col_type(flow_edge/4, from_path, text),
         col_type(flow_edge/4, from_name, text),
         col_type(flow_edge/4, to_path, text),
         col_type(flow_edge/4, to_name, text),
         col_type(flow_reach/4, from_path, text),
         col_type(flow_reach/4, from_name, text),
         col_type(flow_reach/4, to_path, text),
         col_type(flow_reach/4, to_name, text) ],
       [ (flow_edge(FromPath, FromName, ToPath, ToName) <-
            resolved_call_edge(FromPath, FromName, ToPath, ToName)),
         (flow_reach(FromPath, FromName, ToPath, ToName) <-
            flow_edge(FromPath, FromName, ToPath, ToName)),
         (flow_reach(FromPath, FromName, ToPath, ToName) <-
            (flow_reach(FromPath, FromName, MiddlePath, MiddleName),
             flow_edge(MiddlePath, MiddleName, ToPath, ToName))) ]),
  [],
  [ [ +resolved_call_edge(app, entry, lib, decode),
      +resolved_call_edge(lib, decode, sink, persist) ] ],
  [ final(flow_edge/4, [ flow_edge(app, entry, lib, decode),
                         flow_edge(lib, decode, sink, persist) ]),
    final(flow_reach/4, [ flow_reach(app, entry, lib, decode),
                          flow_reach(app, entry, sink, persist),
                          flow_reach(lib, decode, sink, persist) ]) ]).
