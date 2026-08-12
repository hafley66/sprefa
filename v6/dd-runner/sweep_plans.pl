% Emit a dd_plan JSON per conformance fixture into OutDir, bucketing throws.
% swipl -q -l compile/6_isolated_compiler_dd.pl -l sweep_plans.pl -g "sweep(Root,OutDir,Report)" -g halt

sweep(Root, OutDir, Report) :-
    atom_concat(Root, '/v6/prolog/conformance/fixtures/*.pl', Pattern),
    expand_file_name(Pattern, Files),
    setup_call_cleanup(open(Report, write, Stream),
                       forall(member(File, Files), sweep_file(File, OutDir, Stream)),
                       close(Stream)).

sweep_file(File, OutDir, Stream) :-
    fixture_names(File, Names),
    forall(member(Name, Names), sweep_one(File, Name, OutDir, Stream)).

fixture_names(File, Names) :-
    setup_call_cleanup(open(File, read, In), scan_names(In, Names), close(In)).

% A fixture file's own `:- op(...)` lines must run before its later terms read.
scan_names(In, Names) :-
    read_term(In, Term, []),
    (   Term == end_of_file
    ->  Names = []
    ;   Term = (:- Directive)
    ->  catch(call(Directive), _, true), scan_names(In, Names)
    ;   Term = fixture(Name, _, _, _, _)
    ->  Names = [Name | Rest], scan_names(In, Rest)
    ;   scan_names(In, Names)
    ).

sweep_one(File, Name, OutDir, Stream) :-
    format(atom(Path), '~w/~w.json', [OutDir, Name]),
    (   catch(isolated_compiler_dd:write_fixture_dd_plan_json(File, Name, Path), Error, true)
    ->  ( var(Error)
        -> format(Stream, '~w\temitted\tok~n', [Name])
        ;  catch(delete_file(Path), _, true),
           throw_reason(Error, Reason),
           format(Stream, '~w\tthrew\t~w~n', [Name, Reason]) )
    ;   catch(delete_file(Path), _, true),
        format(Stream, '~w\tfailed\tno_solution~n', [Name])
    ),
    flush_output(Stream).

throw_reason(Error, Reason) :-
    (   Error = unsupported_construct(Term)
    ->  ( compound(Term) -> functor(Term, Functor, _) ; Functor = Term ),
        format(atom(Reason), 'unsupported_construct(~w)', [Functor])
    ;   compound(Error)
    ->  functor(Error, Functor, Arity),
        format(atom(Reason), '~w/~w', [Functor, Arity])
    ;   format(atom(Reason), '~w', [Error])
    ).
