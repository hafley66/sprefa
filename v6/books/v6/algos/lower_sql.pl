% lower_sql.pl : one dl rule -> one SELECT. Shared holes across body atoms
% become join conditions (identity ==, never unification =, does the lookup:
% two DIFFERENT holes must not merge). Atoms of the recursive rel read the
% _delta table (semi-naive). A head hole with no body binding fails the
% lowering: range-restriction checking costs zero extra code.
% Run: swipl -q -l lower_sql.pl -g go -g halt

:- use_module(library(lists)).
:- op(1150, xfx, <-).

rule_select(Rule, RecPred, Sql) :-
    copy_term(Rule, Head <- Body),
    body_atoms(Body, Atoms),
    atoms_from(Atoms, RecPred, 0, FromItems, [], Bindings, [], Conds),
    Head =.. [_ | HeadArgs],
    sel_cols(HeadArgs, Bindings, Cols),
    atomic_list_concat(Cols, ', ', SelectList),
    atomic_list_concat(FromItems, ', ', FromList),
    ( Conds == [] -> Where = '1=1'
    ; atomic_list_concat(Conds, ' AND ', Where) ),
    format(atom(Sql), 'SELECT DISTINCT ~w FROM ~w WHERE ~w',
           [SelectList, FromList, Where]).

body_atoms((Atom, Rest), [Atom | Atoms]) :- !, body_atoms(Rest, Atoms).
body_atoms(Atom, [Atom]).

atoms_from([], _, _, [], Binds, Binds, Conds, Conds).
atoms_from([Atom | Rest], RecPred, I, [FromItem | FromRest], B0, B, C0, C) :-
    Atom =.. [Name | Args],
    ( Name == RecPred -> atom_concat(Name, '_delta', Table) ; Table = Name ),
    format(atom(Alias), 'a~w', [I]),
    format(atom(FromItem), '~w ~w', [Table, Alias]),
    args_bind(Args, Alias, 0, B0, B1, C0, C1),
    I1 is I + 1,
    atoms_from(Rest, RecPred, I1, FromRest, B1, B, C1, C).

args_bind([], _, _, B, B, C, C).
args_bind([Arg | Rest], Alias, J, B0, B, C0, C) :-
    format(atom(Col), '~w.c~w', [Alias, J]),
    (   var(Arg)
    ->  (   vlookup(Arg, B0, First)
        ->  format(atom(Cond), '~w = ~w', [Col, First]),
            B1 = B0, C1 = [Cond | C0]
        ;   B1 = [Arg-Col | B0], C1 = C0 )
    ;   number(Arg)
    ->  format(atom(Cond), '~w = ~w', [Col, Arg]), B1 = B0, C1 = [Cond | C0]
    ;   format(atom(Cond), '~w = ''~w''', [Col, Arg]), B1 = B0, C1 = [Cond | C0]
    ),
    J1 is J + 1,
    args_bind(Rest, Alias, J1, B1, B, C1, C).

vlookup(Var, [Key-Col | _], Col) :- Var == Key, !.
vlookup(Var, [_ | Rest], Col) :- vlookup(Var, Rest, Col).

sel_cols([], _, []).
sel_cols([Arg | Rest], Binds, [Col | Cols]) :-
    (   var(Arg)    -> vlookup(Arg, Binds, Col)
    ;   number(Arg) -> Col = Arg
    ;   format(atom(Col), '''~w''', [Arg]) ),
    sel_cols(Rest, Binds, Cols).

check(join_cond, ( rule_select(reach(C) <- (edge(P, C), reach(P)), reach, Sql),
                   sub_atom(Sql, _, _, _, 'reach_delta'),
                   sub_atom(Sql, _, _, _, 'a1.c0 = a0.c0') )).
check(unsafe_fails, ( \+ rule_select(bad(X) <- edge(_, _), none, _),
                      copy_term(X, _) )).

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
