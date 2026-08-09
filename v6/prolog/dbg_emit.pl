:- use_module(lower, [lower_program/2, boot_statements/7]).
:- use_module(emit_ts, [emit_program/5]).
main :-
    F='conformance/fixtures/affinity_drop.pl',
    read_fixture_term(F, 'arrival_affinity_rewrite_keeps_delta', Term, Bindings),
    writeln(found),
    ( catch(( program_plan(Term-Bindings, Plan),
              lower_program(Plan, Lowered),
              Term = fixture(_,_,I,_,_),
              Plan = plan(_, prog(Decls,_), Types, RelPlans, _, _, _, _, _),
              Lowered = lowered(_,_,_,_,LS,_,_,_),
              boot_statements(Mode, Decls, Types, RelPlans, I, LS, Boot),
              call(emit_program, 'arrival_affinity_rewrite_keeps_delta', Plan, Lowered, Boot, Text)),
            E, (writeln('ERR:'), print_term(E,[portray(true)]), nl, fail)
          )
    -> ( var(Text)->writeln(no_text) ; string_length(Text,L), writeln(len=L) )
    ;  writeln(GOAL_FALSE)
    ),
    halt.
