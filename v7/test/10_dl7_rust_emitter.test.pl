:- begin_tests(dl7_rust_emitter).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../src/3_emit/2a_dl7_rust_emitter',
              [render_dl7_rust_file/3]).

test(runtime_schema_exactly_matches_the_soopy_owned_rust_region) :-
    Schema = 'v7/schema/0_runtime_types.dl7',
    Target = 'v6/dd-runner/src/0_dl7_types.rs',
    render_dl7_rust_file(Schema, Generated, Diagnostics),
    read_file_to_string(Target, Actual, []),
    format(string(Expected),
           '// sprefa:auto-begin dl7-runtime-types~n~s// sprefa:auto-end dl7-runtime-types~n',
           [Generated]),
    Observed = generated_rust(Diagnostics, Actual),
    Observed == generated_rust([], Expected).

:- end_tests(dl7_rust_emitter).
