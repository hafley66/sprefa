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

:- end_tests(dl7_dbsp_rust_emitter).
