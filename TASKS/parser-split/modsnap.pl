% swipl -g main -t halt modsnap.pl -- <file.pl> <module> > <module>.listing
% Source positions never reach portray_clause: same clauses, same bytes.

:- use_module(library(lists)).

main :-
    current_prolog_flag(argv, Argv),
    append(_, [File, ModAtom], Argv),
    atom_string(Mod, ModAtom),
    load_files(File, [if(true), must_be_module(true)]),
    own_predicates(Mod, Keys),
    forall(member(Name/Arity, Keys), snap(Mod, Name, Arity)).

own_predicates(Mod, Keys) :-
    findall(Name/Arity,
            ( current_predicate(Mod:Name/Arity),
              functor(Head, Name, Arity),
              predicate_property(Mod:Head, defined),
              \+ predicate_property(Mod:Head, imported_from(_)),
              \+ predicate_property(Mod:Head, foreign)
            ),
            Keys0),
    sort(Keys0, Keys).

snap(Mod, Name, Arity) :-
    format("%%%% ~w/~w~n", [Name, Arity]),
    functor(Head, Name, Arity),
    forall(catch(clause(Mod:Head, Body), _, fail),
           portray_clause((Head :- Body))),
    nl.
