:- begin_tests(dl7_rust_type_emitter).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/2_comptime/2_compiler', [compile_unit/3]).
:- use_module('../src/3_emit/2_rust_type_emitter',
              [render_rust_type_file/4, render_rust_type_rows/4]).

rust_source(
    'v6/sprefa-extract/tests/fixtures/tsi/rust_probe/src/lib.rs').
rust_type_stream('v7/test/fixtures/tsi/5_rust_graph.jsonl').
rust_type_golden('v7/test/fixtures/6_rust_types.expected.dl7').

test(empty_tuple_source_name_is_rendered_verbatim_and_compiles) :-
    Rows = [ extract_run(1, syntax, rust, test, ['unit']),
             extract_fact(1, 'tsi.type', [id(7)]),
             extract_fact(2, 'tsi.name', [id(7), text("()")]),
             extract_witness(1, 1, parse),
             extract_witness(2, 1, parse)
           ],
    render_rust_type_rows('unit.rs', Rows, Text, RenderDiagnostics),
    Text == "; generated from unit.rs\n(: ()\n   (*))\n\n",
    RenderDiagnostics == [],
    dl7_text_unit(generated_unit, generated_unit, Text,
                  Unit, ReadDiagnostics),
    compile_unit(Unit, _, CompileDiagnostics),
    ReadDiagnostics == [],
    CompileDiagnostics == [].

test(rust_product_sum_generic_trait_and_impl_render_and_compile_together) :-
    rust_source(SourcePath),
    rust_type_stream(StreamPath),
    rust_type_golden(GoldenPath),
    render_rust_type_file(SourcePath, StreamPath, Text, RenderDiagnostics),
    render_rust_type_file(SourcePath, StreamPath, TextAgain,
                          SecondRenderDiagnostics),
    read_file_to_string(GoldenPath, Expected, [encoding(utf8)]),
    Text == Expected,
    TextAgain == Text,
    \+ sub_string(Text, _, _, _, "rust_type_"),
    once(sub_string(Text, _, _, _, "(: User\n")),
    once(sub_string(Text, _, _, _, "(: name Option)")),
    once(sub_string(Text, _, _, _, "(rust_trait Mapper)")),
    RenderDiagnostics == [],
    SecondRenderDiagnostics == [],
    dl7_text_unit(generated_rust_types, generated_rust_types, Text,
                  Unit, ReadDiagnostics),
    compile_unit(Unit, Compiled, CompileDiagnostics),
    ReadDiagnostics == [],
    CompileDiagnostics == [],
    Compiled = compiled_unit(_, checked_datalog(root_graph(_, Edges), _, _, _),
                             CompilerRows),
    forall(member(Name, ['Mapper', 'Shape', 'User', 'View']),
           memberchk(':'(_, Name, ref(_), _), Edges)),
    memberchk(call(_, [ref(_), ref(_), ref(_)]), CompilerRows).

:- end_tests(dl7_rust_type_emitter).
