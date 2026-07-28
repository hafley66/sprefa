% lattice.pl : are the existing kind words already named lattices?
%
% The claim under test (a Q1/Q2 extension, not part of the original header):
% the policy bundle gains a MERGE bit, and set / log / keyed are three named
% join-semilattices rather than three separate mechanisms.
%
%   set    = union of rows
%   log    = union of STAMPED rows (engine.pl q1: st(Tick, Seq) is part of the
%            value, which is exactly what makes an append idempotent)
%   keyed  = max on stamp, per key (latest wins)
%
% What the lab actually grades here is our own claim, not anyone else's: that
% each merge is idempotent, commutative and associative (so it is a join),
% that state moves monotonically upward under arrivals, and -- the part that
% matters against R7 -- whether a monotone STATE still produces a retracting
% BOUNDARY delta. Prior-art citations live in the verdict, not here.
%
% State shapes:
%   set    : sorted list of Row
%   log    : sorted list of st(Tick, Seq)-Row
%   keyed  : sorted list of Key-st(Tick, Seq)-Value

:- module(lattice,
          [ merge_kind/2, lub/4, leq/3, merge_all/3, observed/3, boundary/4 ]).

:- use_module(library(lists)).

merge_kind(set,   union_of_rows).
merge_kind(log,   union_of_stamped_rows).
merge_kind(keyed, max_on_stamp_per_key).

% ═══ the joins ══════════════════════════════════════════════════════════════

lub(set, Left, Right, State) :- append(Left, Right, Both), sort(Both, State).
lub(log, Left, Right, State) :- append(Left, Right, Both), sort(Both, State).
lub(keyed, Left, Right, State) :-
    append(Left, Right, Both),
    findall(Key, member(Key-_-_, Both), Keys0), sort(Keys0, Keys),
    findall(Key-Stamp-Value,
            ( member(Key, Keys),
              findall(S-V, member(Key-S-V, Both), Entries),
              msort(Entries, Sorted), last(Sorted, Stamp-Value) ),
            State0),
    sort(State0, State).

leq(Kind, Left, Right) :- lub(Kind, Left, Right, Joined), Joined == Right.

merge_all(Kind, Arrivals, State) :- foldl(join_step(Kind), Arrivals, [], State).

join_step(Kind, Arrival, State0, State) :- lub(Kind, State0, Arrival, State).

% ═══ what an outside reader sees ════════════════════════════════════════════
% observed/3 is the projection a consumer subscribes to: rows, with stamps and
% lattice bookkeeping erased. boundary/4 is the R7 diff of that projection.

observed(set, State, State).
observed(log, State, Rows) :- findall(Row, member(_-Row, State), Rows).
observed(keyed, State, Rows) :-
    findall(row(Key, Value), member(Key-_-Value, State), Rows0), sort(Rows0, Rows).

boundary(Kind, State0, State, Deltas) :-
    observed(Kind, State0, Before),
    observed(Kind, State, After),
    findall(-Row, ( member(Row, Before), \+ memberchk(Row, After) ), Removed),
    findall(+Row, ( member(Row, After), \+ memberchk(Row, Before) ), Added),
    append(Removed, Added, Deltas).
