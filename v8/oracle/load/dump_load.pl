% Freeze every call v7 makes to the three project fact loaders.
%
%   swipl v8/oracle/load/dump_load.pl -- <driver> <out-dir>
%
% Run from the sprefa repository root; v7 resolves its prelude and its fixture
% paths relative to it. V7_DIR selects the v7 tree (default: v7).
%
% Drivers: tests (the four plunit files that call the loaders), compile (the
% two project compiles the extract-loader test performs plus the dl6
% application), all (both).
%
% Every absolute path inside a dumped term is rewritten from the repository
% root to /CASEROOT in BOTH input and expected, so a case is machine
% independent and the substitution is one the loader logic cannot observe.
:- use_module(library(json)).
:- use_module(library(readutil), [read_file_to_string/3]).
:- prolog_load_context(directory, Here),
   atom_concat(Here, '/../eval/json_terms', Helpers),
   consult(Helpers).
:- initialization(main, main).

:- discontiguous run_driver/2.
:- dynamic call_index/1.
:- dynamic out_dir/1.
:- dynamic repo_root/1.
call_index(0).

main([Driver, OutDir]) :-
    assertz(out_dir(OutDir)),
    absolute_file_name('.', Root),
    assertz(repo_root(Root)),
    v7_dir(V7),
    load_subject(V7),
    install_wrappers,
    catch(run_driver(Driver, V7), Error,
          format(user_error, "driver error: ~q~n", [Error])),
    call_index(N),
    format(user_error, "calls: ~d~n", [N]).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   V7 = v7
    ).

load_subject(V7) :-
    atomic_list_concat([V7, '/src/2_comptime/0b_filesystem_grapher'], A),
    atomic_list_concat([V7, '/src/2_comptime/0c_extract_loader'], B),
    atomic_list_concat([V7, '/src/2_comptime/0d_source_fact_loader'], C),
    atomic_list_concat([V7, '/src/2_comptime/2_compiler'], D),
    use_module(A, [install_project_graph/6]),
    use_module(B, [load_tsi_stream/3, load_tsi_text/3, accepted_rows/2,
                   install_tsi_graph/6, tsi_expression_environment/3]),
    use_module(C, [load_source_fact_files/3, install_source_fact_graph/6]),
    use_module(D, [compile_dl7_project/5, compile_dl7_project_rows/6]).

install_wrappers :-
    wrap_predicate(
        dl7_filesystem_grapher:install_project_graph(PgP, PgB0, PgO0, PgB,
                                                     PgO, PgD),
        dump_install_project_graph, PgCall,
        ( PgCall,
          record(install_project_graph,
                 [project-PgP, basements-PgB0, origins-PgO0],
                 [basements-PgB, origins-PgO, diagnostics-PgD]) )),
    wrap_predicate(
        dl7_extract_loader:load_tsi_stream(LsPath, LsRows, LsD),
        dump_load_tsi_stream, LsCall,
        ( LsCall,
          stream_lines(LsPath, LsLines),
          record(load_tsi_lines, [origin-LsPath, lines-LsLines],
                 [rows-LsRows, diagnostics-LsD]) )),
    wrap_predicate(
        dl7_extract_loader:load_tsi_text(LtText, LtRows, LtD),
        dump_load_tsi_text, LtCall,
        ( LtCall,
          text_lines(LtText, LtLines),
          record(load_tsi_lines, [origin-memory, lines-LtLines],
                 [rows-LtRows, diagnostics-LtD]) )),
    wrap_predicate(
        dl7_extract_loader:accepted_rows(ArRows, ArAccepted),
        dump_accepted_rows, ArCall,
        ( ArCall,
          record(accepted_rows, [rows-ArRows], [accepted-ArAccepted]) )),
    wrap_predicate(
        dl7_extract_loader:install_tsi_graph(TgRows, TgB0, TgO0, TgB, TgO,
                                             TgD),
        dump_install_tsi_graph, TgCall,
        ( TgCall,
          record(install_tsi_graph,
                 [rows-TgRows, basements-TgB0, origins-TgO0],
                 [basements-TgB, origins-TgO, diagnostics-TgD]) )),
    wrap_predicate(
        dl7_extract_loader:tsi_expression_environment(TeRows, TeImporters,
                                                      TeEnvironment),
        dump_tsi_expression_environment, TeCall,
        ( TeCall,
          record(tsi_expression_environment,
                 [rows-TeRows, importers-TeImporters],
                 [environment-TeEnvironment]) )),
    wrap_predicate(
        dl7_source_fact_loader:load_source_fact_files(SfPaths, SfRows, SfD),
        dump_load_source_fact_files, SfCall,
        ( SfCall,
          source_fact_texts(SfPaths, SfFiles),
          record(load_source_fact_texts, [files-SfFiles],
                 [rows-SfRows, diagnostics-SfD]) )),
    wrap_predicate(
        dl7_source_fact_loader:install_source_fact_graph(SgRows, SgB0, SgO0,
                                                         SgB, SgO, SgD),
        dump_install_source_fact_graph, SgCall,
        ( SgCall,
          record(install_source_fact_graph,
                 [rows-SgRows, basements-SgB0, origins-SgO0],
                 [basements-SgB, origins-SgO, diagnostics-SgD]) )).

% read_line_to_string/2 drops a trailing \r\n as well as a trailing \n and
% yields the last partial line of a file with no terminator.
stream_lines(Path, Lines) :-
    read_file_to_string(Path, Text, [encoding(utf8)]),
    text_lines(Text, Lines).

text_lines(Text, Lines) :-
    split_string(Text, "\n", "", Parts0),
    drop_final_empty(Parts0, Parts),
    maplist(drop_carriage_return, Parts, Lines).

drop_final_empty(Parts, Kept) :-
    append(Kept, [""], Parts),
    !.
drop_final_empty(Parts, Parts).

drop_carriage_return(Part, Line) :-
    (   string_concat(Line0, "\r", Part)
    ->  Line = Line0
    ;   Line = Part
    ).

source_fact_texts([], []).
source_fact_texts([Path | Paths], [file(Path, Text) | Files]) :-
    (   catch(read_file_to_string(Path, Text, [encoding(utf8)]), _, fail)
    ->  true
    ;   Text = unreadable
    ),
    source_fact_texts(Paths, Files).

record(Call, InputPairs, ExpectedPairs) :-
    retract(call_index(N0)),
    N is N0 + 1,
    assertz(call_index(N)),
    portable(InputPairs, PortableInput),
    portable(ExpectedPairs, PortableExpected),
    pairs_json(PortableInput, InputJsonPairs),
    atom_string(Call, CallText),
    InputJson = [call-CallText | InputJsonPairs],
    pairs_json(PortableExpected, ExpectedJson),
    dict_pairs(InputDict, json, InputJson),
    dict_pairs(ExpectedDict, json, ExpectedJson),
    Dict = json{input: InputDict, expected: ExpectedDict},
    out_dir(OutDir),
    format(atom(OutFile), "~w/~w_~d.json", [OutDir, Call, N0]),
    setup_call_cleanup(
        open(OutFile, write, Stream, [encoding(utf8)]),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)).

pairs_json([], []).
pairs_json([Key-Value | Pairs], [Key-Json | Jsons]) :-
    term_json(Value, Json),
    pairs_json(Pairs, Jsons).

% Rewrite the repository root out of every atom and string in a term.
portable(Term, Portable) :-
    repo_root(Root),
    rewrite(Term, Root, Portable).

rewrite(Term, _, Term) :- var(Term), !.
rewrite(Term, Root, Portable) :-
    atom(Term),
    !,
    rewrite_text(Term, Root, Portable).
rewrite(Term, Root, Portable) :-
    string(Term),
    !,
    rewrite_text(Term, Root, Portable).
rewrite(Term, _, Term) :- \+ compound(Term), !.
rewrite(Term, Root, Portable) :-
    compound_name_arguments(Term, Name, Arguments),
    maplist(rewrite_argument(Root), Arguments, Rewritten),
    compound_name_arguments(Portable, Name, Rewritten).

rewrite_argument(Root, Argument, Rewritten) :-
    rewrite(Argument, Root, Rewritten).

rewrite_text(Text, Root, Rewritten) :-
    (   sub_atom(Text, 0, _, After, Root)
    ->  sub_atom(Text, _, After, 0, Rest),
        atom_concat('/CASEROOT', Rest, Atom),
        (   string(Text)
        ->  atom_string(Atom, Rewritten)
        ;   Rewritten = Atom
        )
    ;   Rewritten = Text
    ).

run_driver(all, V7) :-
    !,
    run_driver(tests, V7),
    run_driver(compile, V7).
run_driver(tests, V7) :-
    !,
    forall(member(Name, ['2_module_system.test', '4_extract_loader.test',
                         '5_source_fact_loader.test', '8_source_query.test']),
           ( atomic_list_concat([V7, '/test/', Name, '.pl'], Path),
             catch(consult(Path), E, format(user_error, "load ~w: ~q~n",
                                            [Path, E])) )),
    ignore(catch(run_tests, E2, format(user_error, "run_tests: ~q~n", [E2]))).
run_driver(compile, V7) :-
    atomic_list_concat([V7, '/test/fixtures/tsi_project'], TsiRoot),
    atomic_list_concat([TsiRoot, '/0_contract.dl7'], Contract),
    atomic_list_concat([V7, '/test/fixtures/tsi/1_semantic_user.jsonl'], S1),
    compile_probe(TsiRoot, [Contract, tsi_streams([S1])]),
    atomic_list_concat([V7, '/examples'], Examples),
    atomic_list_concat([Examples, '/0_rust_traits.dl7'], Traits),
    atomic_list_concat([V7, '/test/fixtures/tsi/5_rust_graph.jsonl'], S2),
    compile_probe(Examples, [Traits, tsi_streams([S2])]),
    atomic_list_concat([V7, '/applications/dl6'], Dl6Root),
    atomic_list_concat([Dl6Root, '/0_catalog.dl7'], Catalog),
    compile_probe(Dl6Root, [Catalog]).

compile_probe(Root, Paths) :-
    catch(compile_dl7_project(Root, Paths, _, _, Diagnostics),
          Error,
          Diagnostics = [caught(Error)]),
    format(user_error, "compile ~w: ~q~n", [Root, Diagnostics]).

% Diagnostic probes. Every code the three modules raise that the natural corpus
% does not reach gets one crafted call here. Run from the v8 worktree root so
% the probe fixture paths under v8/oracle/load/cases/ rewrite to /CASEROOT.
run_driver(probes, _) :-
    forall(probe(Goal), ignore(catch(Goal, E,
                                     format(user_error, "probe: ~q~n", [E])))).

probe(install_project_graph(
          dl7_project('/virtual/project',
                      [dl7_unit(file('/other/tree/0_a.dl7'),
                                content_sha256(probe), [], [], [])]),
          [], [], _, _, _)).
probe(install_project_graph(
          dl7_project('/virtual/project',
                      [dl7_unit(file('/virtual/project/0_src/notes.txt'),
                                content_sha256(probe), [], [], [])]),
          [], [], _, _, _)).
probe(install_project_graph(
          dl7_project('/virtual/project',
                      [dl7_unit(memory, content_sha256(probe), [], [], [])]),
          [], [], _, _, _)).
probe(install_project_graph(
          dl7_project('/virtual/project', [not_a_dl7_unit]), [], [], _, _, _)).
probe(install_project_graph(dl7_project('/virtual/project', []),
                            [], [], _, _, _)).
probe(install_tsi_graph([], [], [], _, _, _)).
probe(install_tsi_graph([extract_fact(1, 'tsi.type', [id(1)])], [], [],
                        _, _, _)).
probe(Goal) :-
    probe_prelude(Stub),
    probe_rows(Rows),
    Goal = install_tsi_graph(Rows, Stub, [], _, _, _).
probe(load_tsi_text("not json at all\n{\"x\": 1}\n\n   \t \n{\"record\": \"protocol\", \"version\": 1}\n", _, _)).
probe(accepted_rows([], _)).
probe(tsi_expression_environment([], [], _)).
probe(tsi_expression_environment([extract_fact(1, 'tsi.type', [id(1)])],
                                 [module(probe)], _)).
probe(load_source_fact_files(
          ['v8/oracle/load/cases/source_facts/0_not_array.json'], _, _)).
probe(load_source_fact_files(
          ['v8/oracle/load/cases/source_facts/2_malformed_envelope.json'],
          _, _)).
probe(Goal) :-
    load_source_fact_files(
        ['v8/oracle/load/cases/source_facts/1_expected.json'], Rows, []),
    Goal = install_source_fact_graph(Rows, [], [], _, _, _).
probe(load_tsi_stream('v8/oracle/load/cases/tsi/0_syntax_user.jsonl', _, _)).
probe(load_tsi_stream('v8/oracle/load/cases/tsi/1_broken.jsonl', _, _)).

probe_prelude(
    [module_basement(
         module(prelude),
         basement_program(
             root_graph([node(module(prelude)), module(module(prelude)),
                         product(module(prelude)),
                         node(prelude_string), product(prelude_string)],
                        [pending_edge(module(prelude), string,
                                      target(prelude_string), 0)]),
             datalog_program([], [], [])))]).

% One stream that raises tsi_primitive_class_absent, tsi_id_unresolved,
% tsi_duplicate_edge_label and tsi_edge_unplaced at once.
probe_rows([ extract_run(1, syntax, probe, test, ['probe']),
             extract_fact(1, 'tsi.type', [id(0)]),
             extract_fact(2, 'tsi.type', [id(1)]),
             extract_fact(3, 'tsi.primitive', [id(1), atom(frobnicate)]),
             extract_fact(4, 'tsi.edge',
                          [id(10), id(0), text("label"), id(1), int(0)]),
             extract_fact(5, 'tsi.edge',
                          [id(11), id(0), text("label"), id(1), int(1)]),
             extract_fact(6, 'tsi.edge',
                          [id(12), id(0), text("gone"), id(99), int(2)]),
             extract_fact(7, 'tsi.name', [id(98), text("absent")]),
             extract_witness(1, 1, parse),
             extract_witness(2, 1, parse),
             extract_witness(3, 1, parse),
             extract_witness(4, 1, parse),
             extract_witness(5, 1, parse),
             extract_witness(6, 1, parse),
             extract_witness(7, 1, parse)
           ]).
