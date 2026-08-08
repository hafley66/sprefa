:- module(dbg_trace, []).
:- use_module(lower, [lower_program/2, boot_statements/6]).
:- use_module(emit_ts, [emit_program/5]).
:- dynamic(seen/1).
main :-
    F='conformance/fixtures/affinity_drop.pl',
    read_fixture_term(F, 'arrival_affinity_rewrite_keeps_delta', Term, Bindings),
    program_plan(Term-Bindings, Plan),
    lower_program(Plan, Lowered),
    Term = fixture(_,_,I,_,_),
    Plan = plan(_, prog(Decls,_), RelPlans, _, _, _, _),
    Lowered = lowered(_,_,_,_,LS,_,_,_),
    boot_statements(Mode, Decls, RelPlans, I, LS, Boot),
    emit_program('x', Plan, Lowered, Boot, Text),
    writeln(OK),
    halt.
