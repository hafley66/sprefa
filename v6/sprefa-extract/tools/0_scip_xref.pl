:- use_module(library(http/json)).
:- use_module(library(prolog_source)).
:- use_module(library(prolog_xref)).

main([Root]) :-
    directory_source_files(Root, Files, [recursive(true), if(true)]),
    forall(member(File, Files), emit_file(Root, File)).

emit_file(Root, File) :-
    catch(xref_source(File, [silent(true)]), _, fail),
    relative_file_name(File, Root, Relative),
    forall(
        ( xref_defined(File, Head, How),
          xref_definition_line(How, Line),
          callable_key(Head, Name, Arity)
        ),
        emit(_{record:def, path:Relative, name:Name, arity:Arity, line:Line})
    ),
    forall(
        ( xref_called(File, Called, By, _, Line),
          callable_key(Called, Name, Arity),
          callable_key(By, CallerName, CallerArity)
        ),
        emit(_{record:ref, path:Relative, name:Name, arity:Arity,
               caller_name:CallerName, caller_arity:CallerArity, line:Line})
    ).

callable_key(Module:Head, Name, Arity) :-
    atom(Module),
    !,
    callable_key(Head, Name, Arity).
callable_key(Head, Name, Arity) :-
    callable(Head),
    functor(Head, Name, Arity).

emit(Dict) :-
    json_write_dict(current_output, Dict, [width(0)]),
    nl.

:- initialization(main, main).
