:- begin_tests(dl7_source_query).

:- use_module('../src/2_comptime/0c_extract_loader', [load_tsi_stream/3]).
:- use_module('../src/4_tool/0_source_query_mainer',
              [query_tsi_rows/5, extract_tsi_rows/4]).

test(a_program_relation_joins_loaded_source_facts) :-
    load_tsi_stream('v7/test/fixtures/tsi/5_rust_graph.jsonl',
                    TsiRows, []),
    query_tsi_rows(
        'v7/examples/0_rust_traits.dl7',
        TsiRows, source_trait, Rows, Diagnostics),
    Diagnostics == [],
    Rows == [[const("Mapper")]].

test(an_absent_result_relation_is_named) :-
    Runtime = checked_datalog(root_graph([], []),
                              datalog_program([], [], []), [], []),
    dl7_source_query_mainer:select_query_rows(
        [], '/program.dl7', absent, [], Runtime, Rows, Diagnostics),
    Rows == [],
    Diagnostics == [diagnostic(query, file('/program.dl7'),
                               unknown_relation(absent))].

test(a_source_list_extracts_every_file_in_one_run) :-
    dl7_source_query_mainer:default_extract(Extract),
    extract_tsi_rows(Extract,
                     ['v7/test/fixtures/ts/0_alpha.ts',
                      'v7/test/fixtures/ts/1_beta.ts'],
                     TsiRows, Diagnostics),
    Diagnostics == [],
    memberchk(extract_fact(_, 'tsi.name', [_, text("Alpha")]), TsiRows),
    memberchk(extract_fact(_, 'tsi.name', [_, text("Beta")]), TsiRows).

:- end_tests(dl7_source_query).
