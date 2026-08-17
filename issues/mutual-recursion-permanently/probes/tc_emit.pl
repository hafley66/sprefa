:- use_module('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/sweep').
:- use_module('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/compile',
              [ program_plan/3, default_intern_mode/1 ]).
:- use_module('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/lower',
              [ lower_program/2, boot_statements/7 ]).
:- use_module('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/emit_ts',
              [ emit_program/5 ]).
:- use_module('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-ab363771c9c609831/v6/prolog/emit_rust',
              [ emit_program/5 as emit_rust_program ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

scratch('/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/9236bad9-763d-49b4-bcdf-000c5fa97d8d/scratchpad').

write_text(Path, Format, Text) :-
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, Format, [Text]),
                       close(Stream)).

emit_all :-
    scratch(Dir),
    atomic_list_concat([Dir, '/tc_probe.pl'], File),
    default_intern_mode(Mode0),
    sweep:read_all_fixtures(File, Entries),
    forall(member(entry(Name, Term, Bindings), Entries),
           emit_one(Dir, Mode0, Name, Term, Bindings)).

emit_one(Dir, InternMode, Name, Term, Bindings) :-
    catch(
        ( program_plan(Term-Bindings, [intern(InternMode)], Plan),
          lower_program(Plan, Lowered),
          Term = fixture(Name, _Prog, Initial, Schedule, _Expectations),
          Plan = plan(_, prog(Decls, _Rules), Types, RelPlans, _, _, _, _, Mode),
          Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
          boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements, Boot),
          emit_ts:emit_program(Name, Plan, Lowered, Boot, TsText),
          atomic_list_concat([Dir, '/', Name, '.ts'], TsPath),
          write_text(TsPath, "~s", TsText),
          emit_rust:emit_program(Name, Plan, Lowered, Boot, RustText),
          atomic_list_concat([Dir, '/', Name, '.rs'], RustPath),
          write_text(RustPath, '~w', RustText),
          sweep:schedule_json(Types, RelPlans, Schedule, ScheduleJson),
          atomic_list_concat([Dir, '/', Name, '.schedule.json'], SchedulePath),
          write_text(SchedulePath, '~w', ScheduleJson),
          format('EMIT ~w compiled~n', [Name])
        ),
        Error,
        format('EMIT ~w FAILED ~q~n', [Name, Error])).
