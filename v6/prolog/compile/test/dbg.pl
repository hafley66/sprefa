:- use_module('../../compile', [ read_fixture_term/4, program_plan/2, compile_dl6/2 ]).
:- use_module('../compile/test/2_subscribe', []).

main :-
    % emulate emitted_module_prunes test
    string_codes(
      "rel seed(id: int, secs: int).\nrel job(id: int, secs: int).\nrel unread(id: int).\njob(Id, Secs) <- seed(Id, Secs).\n? job(7, secs).\n",
      Codes),
    parse_dl(Codes, Program, Bindings, []),
    program_plan(fixture(x, Program, [], [], [])-Bindings, Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _, _, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Mode, Decls, Types, RelPlans, [], LevelStatements, Boot),
    emit_program(x, Plan, Lowered, Boot, Text),
    ( sub_atom(Text, _, _, _, 'subscribed_rels')
    -> writeln('HAS subscribed_rels')
    ; writeln('NO subscribed_rels') ),
    ( sub_atom(Text, _, _, _, 'arrival_targets') -> writeln('HAS arrival_targets') ; writeln('NO arrival_targets') ),
    halt.
:- initialization(main).
