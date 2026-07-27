% magic_sets.pl : demand-driven evaluation as a program transform. The magic
% rel is the subscriber table: the query constant seeds it, one rule per
% recursive call propagates demand upstream, and every original rule gets a
% demand guard. Facts nobody demanded are never derived: prolog's goal stack,
% reified as rows.
% Run: swipl -q -l magic_sets.pl -g go -g halt

:- use_module(library(lists)).

edge(a, b).  edge(b, c).  edge(c, d).
edge(x, y).  edge(y, z).            % a second component, never demanded

:- table path/2, mpath/2, magic/1.

% original program
path(From, To) :- edge(From, To).
path(From, To) :- edge(From, Mid), path(Mid, To).

% the transform, for query pattern path(a, ?)  (adornment bf):
magic(a).                                       % the subscribe, as a fact
magic(Mid) :- magic(From), edge(From, Mid).     % demand propagates upstream
mpath(From, To) :- magic(From), edge(From, To).
mpath(From, To) :- magic(From), edge(From, Mid), mpath(Mid, To).

check(same_answers, ( findall(To, path(a, To), Full0), sort(Full0, Full),
                      findall(To, mpath(a, To), Lazy0), sort(Lazy0, Lazy),
                      Full == Lazy )).
check(demand_set,   ( findall(N, magic(N), Ms0), sort(Ms0, [a, b, c, d]) )).
check(cold_stays_cold, ( \+ mpath(x, _) )).     % undemanded component: no rows

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
