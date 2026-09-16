:- begin_tests(dl7_dbsp_rust_emitter).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../src/3_emit/1b_dbsp_rust_emitter',
              [render_dbsp_rust/3]).

test(native_rust_plan_contains_typed_constructors_without_a_json_program) :-
    compile_dl7('v7/test/fixtures/12_native_runtime.dl7',
                _, Runtime, CompileDiagnostics),
    render_dbsp_rust(Runtime, Text, EmitDiagnostics),
    once(sub_string(Text, _, _, _, 'pub fn relations() -> Vec<Rel>')),
    once(sub_string(Text, _, _, _, 'pub fn ddl() -> Vec<String>')),
    once(sub_string(Text, _, _, _, 'pub fn rules() -> Vec<Rule>')),
    once(sub_string(Text, _, _, _, 'pub fn tick_order() -> Vec<String>')),
    once(sub_string(Text, _, _, _, 'pub fn initial() -> Vec<Row>')),
    once(sub_string(Text, _, _, _, 'pub fn operators() -> Vec<Operator>')),
    once(sub_string(Text, _, _, _, 'Predicate { column_equals:')),
    \+ sub_string(Text, _, _, _, 'PROGRAM_JSON'),
    \+ sub_string(Text, _, _, _, 'from_str'),
    CompileDiagnostics == [],
    EmitDiagnostics == [].

test(native_fixture_exactly_matches_the_soopy_owned_rust_region) :-
    compile_dl7('v7/test/fixtures/12_native_runtime.dl7',
                _, Runtime, CompileDiagnostics),
    render_dbsp_rust(Runtime, Generated, EmitDiagnostics),
    read_file_to_string('v6/dd-runner/src/2_generated_fixture.rs',
                        Actual, []),
    format(string(ExpectedRegion),
           '// sprefa:auto-begin dl7-native-runtime~n~s// sprefa:auto-end dl7-native-runtime~n',
           [Generated]),
    once(sub_string(Actual, _, _, _, ExpectedRegion)),
    CompileDiagnostics == [],
    EmitDiagnostics == [].

int_lt_fixture(checked_datalog(
    root_graph(
        [],
        [ ':'(module(file('/fixture.dl7')), joined,
              ref(owner(file('/fixture.dl7'), relation(0))), 0),
          ':'(module(file('/fixture.dl7')), 'tsi.name',
              ref(tsi_relation(source, 'tsi.name')), 1),
          ':'(owner(file('/fixture.dl7'), relation(0)), name,
              ref(primitive(int)), 0)
        ]),
    datalog_program(
        [ relation(ref(owner(file('/fixture.dl7'), relation(0))), 1, []),
          relation(ref(tsi_relation(source, 'tsi.name')), 2, [])
        ],
        [],
        [ rule(
              call(ref(owner(file('/fixture.dl7'), relation(0))),
                   [var(name)]),
              [ checked_goal(
                    positive,
                    call(ref(tsi_relation(source, 'tsi.name')),
                         [var(identity), var(name)])),
                checked_goal(
                    positive,
                    call(ref(kernel(int_lt)),
                         [var(name), const(5)])),
                checked_goal(
                    positive,
                    call(ref(kernel(int_lt)),
                         [const(0), var(name)]))
              ])
        ]),
        [],
        [])).

test(int_lt_goals_render_native_column_less_than_operands) :-
    int_lt_fixture(Runtime),
    render_dbsp_rust(Runtime, Text, Diagnostics),
    once(sub_string(Text, _, _, _,
                    'Predicate { column_equals: None, literal_equals: None, column_less_than: Some([serde_json::Value::String(String::from("b0.c1")), serde_json::Value::from(5_i64)]) }')),
    once(sub_string(Text, _, _, _,
                    'Predicate { column_equals: None, literal_equals: None, column_less_than: Some([serde_json::Value::from(0_i64), serde_json::Value::String(String::from("b0.c1"))]) }')),
    once(sub_string(Text, _, _, _, 'column_less_than: None')),
    Diagnostics == [].

:- end_tests(dl7_dbsp_rust_emitter).
