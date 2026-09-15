% Freeze every checker call the v7 compiler performs for one .dl7 fixture.
%
%   swipl v8/oracle/check/dump_check.pl -- <fixture.dl7> <out-prefix>
%   swipl v8/oracle/check/dump_check.pl -- bare <case.dl7> <out-prefix>
%
% Run from the sprefa repository root so v7 finds its prelude. V7_DIR selects
% the v7 tree (default: v7). Writes <out-prefix>_<n>.json per checker call.
%
% The default mode drives compile_dl7/4, which prepends the userland type
% prelude; every basement_program therefore carries the whole prelude and one
% case runs about 1.2 MB. The bare mode drives compile_units/3 on the one
% authored unit with no prelude, which is what keeps the per-diagnostic cases
% under oracle/check/cases/ small enough to commit.
%
% Both exported entry points are wrapped. check_datalog/4 takes the lowerer's
% basement_program and origins; check_resolved_rules/5 takes compiler-generated
% checked-IR candidates whose relation references are already canonical.
:- use_module(library(json)).
:- use_module(library(readutil), [read_file_to_string/3]).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- dynamic call_index/1.
call_index(0).

main([bare, File, Prefix]) :-
    !,
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_units/3]),
    atomic_list_concat([V7, '/src/0_reader/2_embedder'], EmbedderPath),
    use_module(EmbedderPath, [dl7_text_unit/5]),
    install_wrappers(Prefix),
    read_file_to_string(File, Text, [encoding(utf8)]),
    dl7_text_unit(file(File), File, Text, Unit, ReaderDiagnostics),
    format(user_error, "reader diagnostics: ~q~n", [ReaderDiagnostics]),
    catch(compile_units([Unit], _Compiled, CompileDiagnostics),
          Error,
          ( CompileDiagnostics = [caught(Error)] )),
    format(user_error, "compile diagnostics: ~q~n", [CompileDiagnostics]),
    call_index(N),
    format(user_error, "checker calls: ~d~n", [N]).
main([Fixture, Prefix]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    install_wrappers(Prefix),
    catch(compile_dl7(Fixture, _Rows, _Runtime, CompileDiagnostics),
          Error,
          ( CompileDiagnostics = [caught(Error)] )),
    format(user_error, "compile diagnostics: ~q~n", [CompileDiagnostics]),
    call_index(N),
    format(user_error, "checker calls: ~d~n", [N]).

install_wrappers(Prefix) :-
    wrap_predicate(dl7_checker:check_datalog(Basement, Origins, Checked,
                                             CheckDiagnostics),
                   dump_check_datalog, Check,
                   ( Check,
                     record_datalog(Prefix, Basement, Origins, Checked,
                                    CheckDiagnostics) )),
    wrap_predicate(dl7_checker:check_resolved_rules(
                       Relations, Rules, Depends, Strata, ResolvedDiagnostics),
                   dump_check_resolved, Resolved,
                   ( Resolved,
                     record_resolved(Prefix, Relations, Rules, Depends,
                                     Strata, ResolvedDiagnostics) )).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

record_datalog(Prefix, Basement, Origins, Checked, Diagnostics) :-
    term_json(Basement, BasementJson),
    term_json(Origins, OriginsJson),
    bound_json(Checked, CheckedJson, CheckedBound),
    term_json(Diagnostics, DiagnosticsJson),
    Dict = _{ entry: check_datalog,
              input: _{ program: BasementJson, origins: OriginsJson },
              expected: _{ checked: CheckedJson, checked_bound: CheckedBound,
                           diagnostics: DiagnosticsJson } },
    write_case(Prefix, Dict, check_datalog).

record_resolved(Prefix, Relations, Rules, Depends, Strata, Diagnostics) :-
    term_json(Relations, RelationsJson),
    term_json(Rules, RulesJson),
    bound_json(Depends, DependsJson, DependsBound),
    bound_json(Strata, StrataJson, StrataBound),
    term_json(Diagnostics, DiagnosticsJson),
    Dict = _{ entry: check_resolved_rules,
              input: _{ relations: RelationsJson, rules: RulesJson },
              expected: _{ depends: DependsJson, depends_bound: DependsBound,
                           strata: StrataJson, strata_bound: StrataBound,
                           diagnostics: DiagnosticsJson } },
    write_case(Prefix, Dict, check_resolved_rules).

write_case(Prefix, Dict, Entry) :-
    retract(call_index(N0)),
    N is N0 + 1,
    assertz(call_index(N)),
    format(atom(OutFile), "~w_~d.json", [Prefix, N0]),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)),
    format(user_error, "~w entry=~w~n", [OutFile, Entry]).

bound_json(Value, null, false) :-
    var(Value),
    !.
bound_json(Value, Json, true) :-
    term_json(Value, Json).
