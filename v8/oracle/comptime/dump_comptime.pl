% swipl v8/oracle/comptime/dump_comptime.pl -- <file.dl7> <out-prefix>
% V7_DIR selects the staged v7 tree; wrap_predicate/4 records on success, so a
% nested call lands at a lower index than its caller.
:- use_module(library(json)).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- dynamic call_index/1.
call_index(0).

main([File, Prefix]) :-
    v7_dir(V7),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], CompilerPath),
    use_module(CompilerPath, [compile_dl7/4]),
    install_wrappers(Prefix),
    catch(compile_dl7(File, _Rows, _Runtime, Diagnostics),
          Error,
          Diagnostics = [caught(Error)]),
    format(user_error, "compile diagnostics: ~q~n", [Diagnostics]),
    call_index(N),
    format(user_error, "comptime calls: ~d~n", [N]).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

install_wrappers(Prefix) :-
    wrap_predicate(
        dl7_compiler:evaluate_checked(_, Checked, Compiled, EvaluationDiagnostics),
        dump_evaluate_checked, Goal,
        ( Goal,
          record_evaluate_checked(Prefix, Checked, Compiled,
                                  EvaluationDiagnostics) )),
    wrap_predicate(
        dl7_compiler:final_checked_program(_, Facts, Generated, Final, FinalDiagnostics),
        dump_final, FinalGoal,
        ( FinalGoal,
          record_refreeze(Prefix, false, Facts, Generated, Final,
                          FinalDiagnostics) )),
    wrap_predicate(
        dl7_compiler:deferred_checked_program(_, DFacts, DGenerated, DChecked,
                                              DDiagnostics),
        dump_deferred, DeferredGoal,
        ( DeferredGoal,
          record_refreeze(Prefix, true, DFacts, DGenerated, DChecked,
                          DDiagnostics) )),
    wrap_predicate(
        dl7_generated_program_assembler:assemble_generated_program(
            Rows, BaseRelations, GeneratedRelations, GeneratedRules,
            AssemblyDiagnostics),
        dump_assemble, AssembleGoal,
        ( AssembleGoal,
          record(Prefix, assemble_generated_program,
                 json([compiler_rows=Rows, base_relations=BaseRelations]),
                 json([generated_relations=GeneratedRelations,
                       generated_rules=GeneratedRules,
                       diagnostics=AssemblyDiagnostics])) )),
    wrap_predicate(
        dl7_host_planner:validate_hosted_relations(
            HostGraph, HostRelations, HostFacts, HostDiagnostics),
        dump_validate_hosted, HostGoal,
        ( HostGoal,
          record_host(Prefix, HostGraph, HostRelations, HostFacts,
                      HostDiagnostics) )),
    wrap_predicate(
        dl7_host_planner:erase_host_planning_rows(
            EraseGraph, ERelations0, ESeeds0, ERules0,
            ERelations, ESeeds, ERules),
        dump_erase_host, EraseGoal,
        ( EraseGoal,
          record_erase(Prefix, EraseGraph, ERelations0, ESeeds0, ERules0,
                       ERelations, ESeeds, ERules) )),
    wrap_predicate(
        dl7_evaluator:validate_functional_rows(KeyRelations, KeyRows,
                                               KeyDiagnostics),
        dump_validate_keys, KeyGoal,
        ( KeyGoal,
          record(Prefix, validate_functional_rows,
                 json([relations=KeyRelations, rows=KeyRows]),
                 json([diagnostics=KeyDiagnostics])) )),
    wrap_predicate(
        dl7_compiler:generated_expression_environment(
            EnvFacts, EnvGenerated, EnvSlots, Environment),
        dump_environment, EnvGoal,
        ( EnvGoal,
          record_environment(Prefix, EnvFacts, EnvGenerated, EnvSlots,
                             Environment) )).

record_host(Prefix, root_graph(Nodes, Edges), Relations, Facts, Diagnostics) :-
    record(Prefix, validate_hosted_relations,
           json([nodes=Nodes, edges=Edges, relations=Relations,
                 compiler_facts=Facts]),
           json([diagnostics=Diagnostics])).

record_erase(Prefix, root_graph(Nodes, Edges), Relations0, Seeds0, Rules0,
             Relations, Seeds, Rules) :-
    record(Prefix, erase_host_planning_rows,
           json([nodes=Nodes, edges=Edges, relations=Relations0,
                 seeds=Seeds0, rules=Rules0]),
           json([relations=Relations, seeds=Seeds, rules=Rules])).

record_environment(Prefix, Facts, Generated, Slots,
                   expression_environment(Reservations, Relations, Edges)) :-
    record(Prefix, generated_expression_environment,
           json([compiler_facts=Facts, generated_relations=Generated,
                 derived_bind_slots=Slots]),
           json([reservations=Reservations, relations=Relations,
                 edges=Edges])).

record_evaluate_checked(Prefix, Checked, Compiled, Diagnostics) :-
    checked_fields(Checked, Fields),
    compiled_json(Compiled, CompiledJson),
    term_json(Diagnostics, DiagnosticsJson),
    dict_pairs(CheckedDict, json, Fields),
    write_case(Prefix, evaluate_checked,
               _{ checked: CheckedDict },
               _{ compiled: CompiledJson, diagnostics: DiagnosticsJson }).

record_refreeze(Prefix, Deferred, Facts, Generated, Checked, Diagnostics) :-
    length(Facts, FactsLength),
    length(Generated, GeneratedLength),
    term_json(Diagnostics, DiagnosticsJson),
    (   checked_fields(Checked, Fields)
    ->  dict_pairs(CheckedDict, json, Fields)
    ;   CheckedDict = null
    ),
    write_case(Prefix, refreeze,
               _{ deferred: Deferred, facts_len: FactsLength,
                  generated_len: GeneratedLength },
               _{ checked: CheckedDict, diagnostics: DiagnosticsJson }).

checked_fields(
    checked_datalog(root_graph(Nodes, Edges),
                    datalog_program(Relations, Seeds, Rules),
                    Depends, Strata),
    [ nodes-NodesJson, edges-EdgesJson, relations-RelationsJson,
      seeds-SeedsJson, rules-RulesJson, depends-DependsJson,
      strata-StrataJson ]) :-
    !,
    term_json(Nodes, NodesJson),
    term_json(Edges, EdgesJson),
    term_json(Relations, RelationsJson),
    term_json(Seeds, SeedsJson),
    term_json(Rules, RulesJson),
    bound_list(Depends, DependsJson),
    bound_list(Strata, StrataJson).

bound_list(Value, []) :- var(Value), !.
bound_list(Value, Json) :- term_json(Value, Json).

compiled_json(compiled_unit(TypeGraphFacts, Runtime, CompilerFacts),
              _{ type_graph_facts: TypeJson, runtime: RuntimeDict,
                 compiler_facts: FactsJson }) :-
    !,
    term_json(TypeGraphFacts, TypeJson),
    checked_fields(Runtime, Fields),
    dict_pairs(RuntimeDict, json, Fields),
    term_json(CompilerFacts, FactsJson).
compiled_json(Compiled, Json) :-
    term_json(Compiled, Json).

record(Prefix, Entry, json(InputPairs), json(OutputPairs)) :-
    maplist(field_json, InputPairs, InputJson),
    maplist(field_json, OutputPairs, OutputJson),
    dict_pairs(Input, json, InputJson),
    dict_pairs(Output, json, OutputJson),
    write_case(Prefix, Entry, Input, Output).

field_json(Key=Value, Key-Json) :-
    (   var(Value)
    ->  Json = []
    ;   term_json(Value, Json)
    ).

write_case(Prefix, Entry, Input, Expected) :-
    retract(call_index(N0)),
    N is N0 + 1,
    assertz(call_index(N)),
    format(atom(OutFile), "~w-~w-~d.json", [Prefix, Entry, N0]),
    setup_call_cleanup(
        open(OutFile, write, Stream),
        json_write_dict(Stream,
                        _{ entry: Entry, input: Input, expected: Expected },
                        [width(0)]),
        close(Stream)),
    format(user_error, "~w entry=~w~n", [OutFile, Entry]).
