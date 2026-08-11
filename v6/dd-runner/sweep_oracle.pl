% Write each fixture's oracle tick log to OutDir/<name>.oracle.jsonl.
% swipl -q -l conformance/ticklog.pl -l sweep_oracle.pl -g "sweep_oracle(OutDir,Report)" -g halt

sweep_oracle(OutDir, Report) :-
    findall(Name, fixture(Name, _, _, _, _), Names0),
    sort(Names0, Names),
    setup_call_cleanup(open(Report, write, Stream),
                       forall(member(Name, Names), oracle_one(Name, OutDir, Stream)),
                       close(Stream)).

oracle_one(Name, OutDir, Stream) :-
    format(atom(Path), '~w/~w.oracle.jsonl', [OutDir, Name]),
    fixture(Name, Prog, Initial, Schedule, _),
    (   catch(setup_call_cleanup(open(Path, write, Out),
                                 with_output_to(Out, print_ticklog(Prog, Initial, Schedule)),
                                 close(Out)),
              Error, true)
    ->  ( var(Error)
        -> format(Stream, '~w\tok~n', [Name])
        ;  catch(delete_file(Path), _, true),
           throw_reason(Error, Reason),
           format(Stream, '~w\tthrew\t~w~n', [Name, Reason]) )
    ;   catch(delete_file(Path), _, true),
        format(Stream, '~w\tfailed~n', [Name])
    ),
    flush_output(Stream).

throw_reason(Error, Reason) :-
    (   compound(Error)
    ->  functor(Error, Functor, Arity),
        format(atom(Reason), '~w/~w', [Functor, Arity])
    ;   format(atom(Reason), '~w', [Error])
    ).
