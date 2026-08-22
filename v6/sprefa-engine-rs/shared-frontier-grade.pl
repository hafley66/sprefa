:- module(shared_frontier_grade, [ generate/2 ]).

:- use_module('../prolog/compile', [ program_plan/3 ]).
:- use_module('../prolog/lower',
              [ lower_program/2, boot_statements/7, with_frontier_mode/2 ]).
:- use_module('../prolog/emit_rust', [ emit_program/5 ]).

generate(OutDir, Report) :-
    make_directory_path(OutDir),
    sweep:fixture_files(Files),
    setup_call_cleanup(open(Report, write, Stream),
                       forall(member(File, Files), generate_file(File, OutDir, Stream)),
                       close(Stream)).

generate_file(File, OutDir, Stream) :-
    sweep:read_all_fixtures(File, Entries),
    forall(member(entry(Name, Term, Bindings), Entries),
           generate_one(Name, Term, Bindings, OutDir, Stream)).

% Differs from rust_grade:generate_one/5 only by the with_frontier_mode wrap,
% which turns a guard stop into an unsupported row carrying its reason.
generate_one(Name, Term, Bindings, OutDir, Stream) :-
    catch(
        ( ( program_plan(Term-Bindings, [], Plan),
            with_frontier_mode(shared, lower_program(Plan, Lowered)),
            Term = fixture(Name, _Program, Initial, Schedule, _Expectations),
            Plan = plan(_, prog(Decls, _Rules), Types, RelPlans, _, _, _, _, Mode),
            Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
            boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements, Boot),
            with_frontier_mode(shared, emit_program(Name, Plan, Lowered, Boot, Text)),
            format(atom(RustPath), '~w/~w.rs', [OutDir, Name]),
            setup_call_cleanup(open(RustPath, write, RustStream),
                               format(RustStream, '~w', [Text]), close(RustStream)),
            sweep:schedule_json(Types, RelPlans, Schedule, ScheduleJson),
            format(atom(SchedulePath), '~w/~w.schedule.json', [OutDir, Name]),
            setup_call_cleanup(open(SchedulePath, write, ScheduleStream),
                               format(ScheduleStream, '~w', [ScheduleJson]), close(ScheduleStream)),
            Result = compiled,
            Reason = ''
          ) -> true
        ; Result = unsupported,
          Reason = 'emitter returned false'
        ),
        Error,
        result_name(Error, Result, Reason)),
    format(Stream, '~w\t~w\t~w~n', [Name, Result, Reason]),
    flush_output(Stream).

result_name(unsupported_construct(Reason), unsupported, Text) :- !,
    term_string(Reason, Text, [numbervars(true)]).
result_name(Error, error, Text) :-
    term_string(Error, Text, [numbervars(true)]).
