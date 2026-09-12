% Freeze every reifier, grapher and artifact-emitter call v7 performs.
%
%   swipl v8/oracle/reify/dump_reify.pl -- compile <fixture.dl7> <out-prefix>
%   swipl v8/oracle/reify/dump_reify.pl -- test <suite.test.pl> <out-prefix>
%   swipl v8/oracle/reify/dump_reify.pl -- case <case.pl> <out-prefix>
%
% Run from the sprefa repository root so v7 finds its prelude and its relative
% fixture paths. V7_DIR selects the v7 tree (default: v7). Writes
% <out-prefix>_<n>.json per wrapped call.
%
% compile mode drives compile_dl7/4 over one fixture. The compiler itself never
% reaches src/3_emit, so that mode measures zero and is kept only as the
% receipt for that claim. test mode consults one v7 plunit suite and runs it;
% the suites are the only v7 callers of emit_compiled/4. case mode consults one
% authored cases/*.pl and calls its case/0; those carry one tiny checked program
% so every diagnostic reason gets a committed case of a few kilobytes.
%
% Seven wrapped entry points, all exported:
%   logical_program_rows/2          reify checked Datalog into compiler rows
%   logical_program_calls/4,5       rows as ground DL7 calls
%   logical_program_rows_calls/5    the same over an existing row view
%   logical_program_graph_calls/3   node/product/':' normalization
%   compiler_view/2                 the immutable emitter input shape
%   emit_compiled/4                 the emitter front door
:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- dynamic call_index/1.
call_index(0).

main([compile, Fixture, Prefix]) :-
    !,
    load_emit_modules,
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    install_wrappers(Prefix),
    catch(compile_dl7(Fixture, _Rows, _Runtime, CompileDiagnostics),
          Error,
          CompileDiagnostics = [caught(Error)]),
    format(user_error, "compile diagnostics: ~q~n", [CompileDiagnostics]),
    report.
main([case, CaseFile, Prefix]) :-
    !,
    load_emit_modules,
    install_wrappers(Prefix),
    consult(CaseFile),
    (   catch(case, Error, ( print_message(error, Error), fail ))
    ->  true
    ;   format(user_error, "case failed~n", [])
    ),
    report.
main([test, Suite, Prefix]) :-
    load_emit_modules,
    install_wrappers(Prefix),
    consult(Suite),
    catch(run_tests, Error, print_message(error, Error)),
    report.

report :-
    call_index(N),
    format(user_error, "reify calls: ~d~n", [N]).

load_emit_modules :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/3_emit/0_logical_program_reifier'], A),
    atomic_list_concat([V7, '/src/3_emit/0a_logical_program_grapher'], B),
    atomic_list_concat([V7, '/src/3_emit/1_artifact_emitter'], C),
    use_module(A),
    use_module(B),
    use_module(C).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

% copy_term before the call so an output argument that the caller passed
% partially instantiated is recorded as the caller wrote it, never as the
% predicate left it.
install_wrappers(Prefix) :-
    wrap_predicate(
        dl7_logical_program_reifier:logical_program_rows(Checked, Rows),
        dump_rows, RowsClosure,
        ( copy_term(Checked, CheckedIn),
          RowsClosure,
          record_rows(Prefix, CheckedIn, Rows) )),
    wrap_predicate(
        dl7_logical_program_reifier:logical_program_calls(
            Facts4, Checked4, Calls4, Diagnostics4),
        dump_calls4, Calls4Closure,
        ( copy_term(Facts4-Checked4, Facts4In-Checked4In),
          Calls4Closure,
          record_calls(Prefix, logical_program_calls_4, Facts4In, Checked4In,
                       all, Calls4, Diagnostics4) )),
    wrap_predicate(
        dl7_logical_program_reifier:logical_program_calls(
            Facts5, Checked5, Relations5, Calls5, Diagnostics5),
        dump_calls5, Calls5Closure,
        ( copy_term(Facts5-Checked5-Relations5,
                    Facts5In-Checked5In-Relations5In),
          Calls5Closure,
          record_calls(Prefix, logical_program_calls_5, Facts5In, Checked5In,
                       Relations5In, Calls5, Diagnostics5) )),
    wrap_predicate(
        dl7_logical_program_reifier:logical_program_rows_calls(
            FactsR, RowsR, RelationsR, CallsR, DiagnosticsR),
        dump_rows_calls, RowsCallsClosure,
        ( copy_term(FactsR-RowsR-RelationsR, FactsRIn-RowsRIn-RelationsRIn),
          RowsCallsClosure,
          record_rows_calls(Prefix, FactsRIn, RowsRIn, RelationsRIn,
                            CallsR, DiagnosticsR) )),
    wrap_predicate(
        dl7_logical_program_grapher:logical_program_graph_calls(
            CheckedG, RelationsG, CallsG),
        dump_graph_calls, GraphClosure,
        ( copy_term(CheckedG-RelationsG, CheckedGIn-RelationsGIn),
          GraphClosure,
          record_graph_calls(Prefix, CheckedGIn, RelationsGIn, CallsG) )),
    wrap_predicate(
        dl7_artifact_emitter:compiler_view(UnitV, View),
        dump_compiler_view, ViewClosure,
        ( copy_term(UnitV, UnitVIn),
          ViewClosure,
          record_compiler_view(Prefix, UnitVIn, View) )),
    wrap_predicate(
        dl7_artifact_emitter:emit_compiled(
            Emitter, UnitE, Artifact, DiagnosticsE),
        dump_emit_compiled, EmitClosure,
        ( copy_term(Emitter-UnitE, EmitterIn-UnitEIn),
          EmitClosure,
          record_emit_compiled(Prefix, EmitterIn, UnitEIn, Artifact,
                               DiagnosticsE) )).

record_rows(Prefix, Checked, Rows) :-
    term_json(Checked, CheckedJson),
    term_json(Rows, RowsJson),
    write_case(Prefix,
               _{ entry: logical_program_rows,
                  input: _{ checked: CheckedJson },
                  expected: _{ rows: RowsJson } }).

record_calls(Prefix, Entry, Facts, Checked, Relations, Calls, Diagnostics) :-
    term_json(Facts, FactsJson),
    term_json(Checked, CheckedJson),
    term_json(Relations, RelationsJson),
    term_json(Calls, CallsJson),
    term_json(Diagnostics, DiagnosticsJson),
    write_case(Prefix,
               _{ entry: Entry,
                  input: _{ compiler_facts: FactsJson,
                            checked: CheckedJson,
                            relations: RelationsJson },
                  expected: _{ calls: CallsJson,
                               diagnostics: DiagnosticsJson } }).

record_rows_calls(Prefix, Facts, Rows, Relations, Calls, Diagnostics) :-
    term_json(Facts, FactsJson),
    term_json(Rows, RowsJson),
    term_json(Relations, RelationsJson),
    term_json(Calls, CallsJson),
    term_json(Diagnostics, DiagnosticsJson),
    write_case(Prefix,
               _{ entry: logical_program_rows_calls,
                  input: _{ compiler_facts: FactsJson,
                            rows: RowsJson,
                            relations: RelationsJson },
                  expected: _{ calls: CallsJson,
                               diagnostics: DiagnosticsJson } }).

record_graph_calls(Prefix, Checked, Relations, Calls) :-
    term_json(Checked, CheckedJson),
    term_json(Relations, RelationsJson),
    term_json(Calls, CallsJson),
    write_case(Prefix,
               _{ entry: logical_program_graph_calls,
                  input: _{ checked: CheckedJson,
                            relations: RelationsJson },
                  expected: _{ calls: CallsJson } }).

record_compiler_view(Prefix,
                     compiled_unit(TypeGraph, Runtime, Facts),
                     compiler_view(ViewTypeGraph, ViewFacts, LogicalRows,
                                   ViewRuntime)) :-
    term_json(TypeGraph, TypeGraphJson),
    term_json(Runtime, RuntimeJson),
    term_json(Facts, FactsJson),
    term_json(ViewTypeGraph, ViewTypeGraphJson),
    term_json(ViewFacts, ViewFactsJson),
    term_json(LogicalRows, LogicalRowsJson),
    term_json(ViewRuntime, ViewRuntimeJson),
    write_case(Prefix,
               _{ entry: compiler_view,
                  input: _{ type_graph_facts: TypeGraphJson,
                            runtime_program: RuntimeJson,
                            compiler_facts: FactsJson },
                  expected: _{ type_graph_facts: ViewTypeGraphJson,
                               compiler_facts: ViewFactsJson,
                               logical_program_rows: LogicalRowsJson,
                               runtime_program: ViewRuntimeJson } }).

record_emit_compiled(Prefix, Emitter,
                     compiled_unit(TypeGraph, Runtime, Facts),
                     Artifact, Diagnostics) :-
    term_json(Emitter, EmitterJson),
    term_json(TypeGraph, TypeGraphJson),
    term_json(Runtime, RuntimeJson),
    term_json(Facts, FactsJson),
    term_json(Artifact, ArtifactJson),
    term_json(Diagnostics, DiagnosticsJson),
    write_case(Prefix,
               _{ entry: emit_compiled,
                  input: _{ emitter: EmitterJson,
                            type_graph_facts: TypeGraphJson,
                            runtime_program: RuntimeJson,
                            compiler_facts: FactsJson },
                  expected: _{ artifact: ArtifactJson,
                               diagnostics: DiagnosticsJson } }).

write_case(Prefix, Dict) :-
    retract(call_index(N0)),
    N is N0 + 1,
    assertz(call_index(N)),
    format(atom(OutFile), "~w_~d.json", [Prefix, N0]),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)),
    get_dict(entry, Dict, Entry),
    format(user_error, "~w entry=~w~n", [OutFile, Entry]).
