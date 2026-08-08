:- module(dbg_bisect, []).
:- use_module(lower, [lower_program/2, boot_statements/6]).
:- use_module(emit_ts, [emit_program/5]).
main :-
    F='conformance/fixtures/affinity_drop.pl',
    read_fixture_term(F, 'arrival_affinity_rewrite_keeps_delta', Term, Bindings),
    writeln(a_parsed),
    program_plan(Term-Bindings, Plan), writeln(b_plan),
    lower_program(Plan, Lowered), writeln(c_lower),
    Lowered = lowered(Name, Ddl, Arr, Edge, Level, Delta, RelP, Targ),
    writeln(d_destructure),
    Plan = plan(_, prog(Decls,_), RelPlans, _, _, _, _),
    Lowered = lowered(_,_,_,_,LevelStmts,_,_,_),
    boot_statements(Mode, Decls, RelPlans, [], LevelStmts, Boot), writeln(e_boot),
    writeln(about_to_emit),
    catch(call(emit_ts:emit_program, Name, Plan, Lowered, Boot, Text), E, (print_term(E,[portrait(true)]),nl,fail)), writeln(f_emit),
    string(Text), writeln(g_string).
