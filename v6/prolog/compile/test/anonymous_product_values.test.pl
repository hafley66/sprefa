% Anonymous product values use the same relation-value and struct planes as
% named products.  These tests start at authored source and retain only the
% product-specific assertions: contextual object construction, nested values,
% catalog reachability, and emitted ProgramJson metadata.

:- begin_tests(anonymous_product_values).

:- use_module('../../compile', [ program_plan/2 ]).
:- use_module('../../next/0_parse/parse_dl_dcg', [ parse_dl/4 ]).
:- use_module('../../next/1_expand/1_expansion', [ expand_program_with_bindings/4 ]).
:- use_module('../../0_type_plane', [ relation_value_object/4 ]).
:- use_module('../../conformance/engine', [ run_program/5 ]).
:- use_module('../../lower', [ lower_program/2, boot_statements/7,
                                catalog_type_rows/6 ]).
:- use_module('../../emit_ts', [ emit_program/5 ]).
:- use_module('../../emit_rust', [ emit_program/5 as emit_rust_program ]).
:- use_module('../../compile/4_emit_jsonschema',
              [ jsonschema_text/3, option_rows/3 ]).
:- use_module('../../compile/7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('../../compile/8_emit_rust_types', [ rust_types_text/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

plan_text(Text, Plan) :-
    parse_text(Text, Program, Bindings),
    program_plan(fixture(anonymous_product_values, Program, [], [], [])-
                 Bindings, Plan).

artifacts(Text, Plan, Rows, Ts, Rust, JsonSchema, ProgramTs, ProgramRust) :-
    plan_text(Text, Plan),
    Plan = plan(_, prog(Decls, Rules), Types, RelPlans, _, _, _, _, Mode),
    catalog_type_rows(Mode, anonymous_product_values, Rules, RelPlans,
                      Decls, Rows0),
    option_rows(Decls, Rows0, Rows),
    ts_types_text(anonymous_product_values, Rows, Ts),
    rust_types_text(anonymous_product_values, Rows, Rust),
    jsonschema_text(anonymous_product_values, Rows, JsonSchema),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    boot_statements(Mode, Decls, Types, RelPlans, [], LevelStatements, Boot),
    emit_program(anonymous_product_values, Plan, Lowered, Boot, ProgramTs),
    emit_rust_program(anonymous_product_values, Plan, Lowered, Boot,
                      ProgramRust).

anonymous_source(Text) :-
    string_concat("rel source(input: text, a: int, b: text).\n",
                  "rel resident(input: text, result: (a: int, b: text)).\n",
                  Prefix),
    string_concat(Prefix,
                  "resident(Input, {a: A, b: B}) <- source(Input, A, B).\n",
                  Text).

test(authored_object_constructs_and_matches) :-
    anonymous_source(Text),
    parse_text(Text, Program, _Bindings),
    once(run_program(Program, [source(hello, 1, world)], [], Final, _)),
    memberchk(resident(hello, obj([a-1, b-world])), Final),
    once(run_program(Program,
                    [resident(hello, obj([b-world, a-1]))], [], IngressFinal,
                    _)),
    memberchk(resident(hello, obj([a-1, b-world])), IngressFinal),
    % The same object spelling is accepted as a world value for the generated
    % product column, so ingress and a rule-built value share one canonical row.
    parse_text("rel resident(input: text, result: (a: int, b: text)).\n", Ingress,
               IngressBindings),
    expand_program_with_bindings(Ingress, IngressBindings,
                                 prog(Decls, _), _),
    once(member(type_decl(Name, [col(a, int), col(b, text)]), Decls)),
    relation_value_object([type_def(Name, [a, b], [int, text])], Name,
                          obj([b-world, a-1]), obj([a-1, b-world])), !.

test(nested_anonymous_products_are_canonical_objects) :-
    string_concat("rel source(a: int, b: text, label: text).\n",
                  "rel holder(value: (inner: (a: int, b: text), label: text)).\n",
                  Prefix),
    string_concat(Prefix,
                  "holder({inner: {a: A, b: B}, label: Label}) <- source(A, B, Label).\n",
                  Text),
    parse_text(Text, Program, _),
    run_program(Program, [source(1, world, tag)], [], Final, _),
    memberchk(holder(obj([inner-obj([a-1, b-world]), label-tag])), Final).

test(named_and_anonymous_products_have_same_field_values) :-
    string_concat("rel source(a: int, b: text).\n",
                  "rel pair(a: int, b: text).\n",
                  Prefix1),
    string_concat(Prefix1,
                  "rel named(value: pair).\n",
                  Prefix2),
    string_concat(Prefix2,
                  "rel anonymous(value: (a: int, b: text)).\n",
                  Prefix3),
    string_concat(Prefix3,
                  "named(pair(A, B)) <- source(A, B).\n",
                  Prefix4),
    string_concat(Prefix4,
                  "anonymous({a: A, b: B}) <- source(A, B).\n",
                  Text),
    parse_text(Text, Program, _),
    run_program(Program, [source(1, world), pair(1, world)], [], Final, _),
    memberchk(named(obj([a-1, b-world])), Final),
    memberchk(anonymous(obj([a-1, b-world])), Final).

test(incomplete_anonymous_context_is_named_refusal) :-
    string_concat("rel source(input: text, a: int).\n",
                  "rel resident(input: text, result: (a: int, b: text)).\n",
                  Prefix),
    string_concat(Prefix,
                  "resident(Input, {a: A}) <- source(Input, A).\n",
                  Text),
    parse_text(Text, Program, _),
    catch(run_program(Program, [source(hello, 1)], [], _, _), Error, true),
    Error = relation_pattern_not_a_relation_value(resident/2, result,
                                                  TypeName, _),
    sub_atom(TypeName, 0, _, _, '__anon_resident_result_').

test(list_and_option_wrappers_materialize_and_execute) :-
    ListText = "rel holder(value: list((a: int, b: text))).\n",
    plan_text(ListText, ListPlan),
    ListPlan = plan(_, prog(ListDecls, _), _, _, _, _, _, _, _),
    member(col_type(holder/1, value, list(ListType)), ListDecls),
    member(type_decl(ListType, [col(a, int), col(b, text)]), ListDecls),
    parse_text(ListText, ListProgram, _),
    run_program(ListProgram, [], [], [], _),
    OptionText = "rel holder(value: option((a: int, b: text))).\n",
    plan_text(OptionText, OptionPlan),
    OptionPlan = plan(_, prog(OptionDecls, _), _, _, _, _, _, _, _),
    member(option_column(holder/1, value, OptionType), OptionDecls),
    member(type_decl(OptionType, [col(a, int), col(b, text)]), OptionDecls),
    parse_text(OptionText, OptionProgram, _),
    run_program(OptionProgram, [], [], [], _), !.

test(obj_relation_name_is_reserved_for_runtime_values) :-
    Text = "rel obj(value: text).\n",
    parse_text(Text, Program, Bindings),
    catch(program_plan(fixture(anonymous_product_values, Program, [], [], [])-
                       Bindings, _), CompilerError, true),
    CompilerError = unsupported_construct(
        reserved_relation_value_carrier(obj/1)),
    catch(run_program(Program, [], [], _, _), EngineError, true),
    EngineError = reserved_relation_value_carrier(obj/1).

test(generated_product_reaches_all_type_emitters_and_program_json) :-
    anonymous_source(Text),
    once(artifacts(Text, Rows, Ts, Rust, JsonSchema, ProgramTs, ProgramRust)),
    once(member(row(GeneratedId, _, _, GeneratedName, rel, _, _, _, _, _, _),
                Rows)),
    sub_atom(GeneratedName, 0, _, _, '__anon_resident_result_'),
    memberchk(row(_, GeneratedId, _, _, concrete_type, _, _, _, _, _, _), Rows),
    sub_atom(Ts, _, _, _, 'export interface AnonResidentResult'),
    sub_atom(Ts, _, _, _, 'result: AnonResidentResult'),
    sub_atom(Rust, _, _, _, 'pub struct AnonResidentResult'),
    sub_atom(Rust, _, _, _, 'result: AnonResidentResult'),
    sub_atom(JsonSchema, _, _, _, '__anon_resident_result_'),
    sub_atom(ProgramTs, _, _, _, 'STRUCT_TYPES'),
    sub_atom(ProgramTs, _, _, _, '__anon_resident_result_'),
    sub_atom(ProgramRust, _, _, _, '"struct_types"'),
    sub_atom(ProgramRust, _, _, _, '__anon_resident_result_'), !.

artifacts(Text, Rows, Ts, Rust, JsonSchema, ProgramTs, ProgramRust) :-
    artifacts(Text, _Plan, Rows, Ts, Rust, JsonSchema, ProgramTs, ProgramRust).

:- end_tests(anonymous_product_values).
