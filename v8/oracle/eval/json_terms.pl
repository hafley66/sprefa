% Term-to-JSON helpers shared by dump_eval.pl and dump_compile.pl.

term_json(T, T) :- integer(T), !.
term_json(T, J) :- is_list(T), !, maplist(term_json, T, J).
% SWI's JSON writer turns the atoms null, true and false into literals; spell
% them as strings so the reader gets the atom back.
term_json(T, _{a: A}) :- atom(T), !, ( memberchk(T, [null, true, false]) -> atom_string(T, A) ; A = T ).
term_json(T, _{s: S}) :- string(T), !, S = T.
term_json(T, _{f: N, args: Js}) :-
    compound(T), !,
    compound_name_arguments(T, N, As),
    maplist(term_json, As, Js).

arg_json(var(I), _{v: J}) :- !, term_json(I, J).
arg_json(aggregate(count, A), _{count: J}) :- !, arg_json(A, J).
arg_json(T, J) :- term_json(T, J).

call_json(call(Rel, Args), _{rel: R, args: As}) :-
    term_json(Rel, R),
    maplist(arg_json, Args, As).

goal_json(checked_goal(Polarity, call(Rel, Args)),
          _{polarity: Polarity, rel: R, args: As}) :-
    term_json(Rel, R),
    maplist(arg_json, Args, As).

rule_json(rule(Head, Goals), _{head: H, body: B}) :-
    call_json(Head, H),
    maplist(goal_json, Goals, B).

diagnostic_json(diagnostic(Phase, none, Payload), _{phase: Phase, payload: J}) :-
    term_json(Payload, J).
