% anonymous_sum_values.test.pl : public artifacts for owner-scoped sum values.

:- begin_tests(anonymous_sum_values).

:- use_module('../../compile', [program_plan/3]).
:- use_module('../../next/2_lower/lower', [catalog_decl_rows/6]).
:- use_module('../../next/2_lower/lower', [lower_program/2, boot_statements/7]).
:- use_module('../../emit_rust', [emit_program/5]).
:- use_module('../../emit_ts', [emit_program/5 as emit_ts_program]).
:- use_module('../4_emit_jsonschema', [option_rows/3, jsonschema_text/3]).
:- use_module('../7_emit_ts_types', [ts_types_text/3]).
:- use_module('../8_emit_rust_types', [rust_types_text/3]).

anonymous_sum_rows(Program, Decls, Rows) :-
    program_plan(fixture(anonymous_sum_values, Program, [], [], [])-[],
                 [intern(dict)],
                 plan(_, prog(Decls, Rules), _, RelPlans, _, _, _, _, _)),
    catalog_decl_rows(anonymous_sum_values, Rules, RelPlans, Decls, Rows0, _),
    option_rows(Decls, Rows0, Rows).

test(owner_sum_keeps_integer_storage_and_tagged_artifacts) :-
    Program = prog([
        col_type(resident/2, id, int),
        col_type(resident/2, result,
                 sum_type([variant(ok, [field(value, option(option(text)))]),
                           variant(err, [field(reason, list(int))])])),
        keyed(resident/2, [1])
    ], []),
    anonymous_sum_rows(Program, Decls, Rows),
    member(enum_column(resident/2, result, EnumName), Decls),
    sub_atom(EnumName, 0, _, _, '__anon_'),
    member(col_type(resident/2, result, int), Decls),
    atomic_list_concat([EnumName, ok], '_', OkName),
    member(col_type(OkName/2, value, int), Decls),
    member(enum_option_payload(EnumName, ok, value, option(text)), Decls),
    ts_types_text(anonymous_sum_values, Rows, Ts),
    sub_string(Ts, _, _, _, "export type AnonResidentResult"),
    sub_string(Ts, _, _, _, "{ tag: 'ok'; value: Option<Option<string>>; }"),
    sub_string(Ts, _, _, _, "{ tag: 'err'; reason: Array<number>; }"),
    rust_types_text(anonymous_sum_values, Rows, Rust),
    sub_string(Rust, _, _, _, "pub enum AnonResidentResult"),
    sub_string(Rust, _, _, _, "Ok { value: DlOption<DlOption<String>> },"),
    sub_string(Rust, _, _, _, "Err { reason: Vec<i64> },"),
    jsonschema_text(anonymous_sum_values, Rows, Schema),
    sub_string(Schema, _, _, _, '"const":"ok"'),
    sub_string(Schema, _, _, _, '"const":"err"'),
    sub_string(Schema, _, _, _, '"items": {"type":"integer"}'),
    sub_string(Schema, _, _, _, '"const":"none"'),
    sub_string(Schema, _, _, _, '"const":"some"').

test(emits_tagged_runtime_schema_plan) :-
    Program = prog([
        col_type(resident/2, id, int),
        col_type(resident/2, result,
                 sum_type([variant(ok, [field(value, text)]),
                           variant(err, [field(reason, int)])])),
        keyed(resident/2, [1])
    ], []),
    program_plan(fixture(anonymous_sum_values, Program, [], [], [])-[],
                 [intern(dict)], Plan),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _, _, _, _, Mode),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, _, Levels, _, _, _),
    boot_statements(Mode, Decls, Types, RelPlans, [], Levels, Boot),
    emit_program(anonymous_sum_values, Plan, Lowered, Boot, Rust),
    emit_ts_program(anonymous_sum_values, Plan, Lowered, Boot, Ts),
    sub_string(Rust, _, _, _, '"enum_types"'),
    sub_string(Rust, _, _, _, '"enum_ref_columns"'),
    sub_string(Rust, _, _, _, '"tag":"ok"'),
    sub_string(Ts, _, _, _, 'export const ENUM_TYPES'),
    sub_string(Ts, _, _, _, 'EnumPlane.intern').

test(option_sum_emits_both_runtime_plans_without_tag_views_as_variants) :-
    Program = prog([
        col_type(resident/2, id, int),
        col_type(resident/2, result,
                 option(sum_type([variant(ok, [field(value, text)]),
                                  variant(err, [])]))),
        keyed(resident/2, [1])
    ], []),
    program_plan(fixture(anonymous_sum_option, Program, [], [], [])-[],
                 [intern(dict)], Plan),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _, _, _, _, Mode),
    member(option_column(resident/2, result, InnerEnum), Decls),
    atom_concat('__opt_', InnerEnum, OptionEnum),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, _, _, _, Levels, _, _, _),
    boot_statements(Mode, Decls, Types, RelPlans, [], Levels, Boot),
    emit_program(anonymous_sum_option, Plan, Lowered, Boot, Rust),
    emit_ts_program(anonymous_sum_option, Plan, Lowered, Boot, Ts),
    format(string(OptionName), '"name":"~w"', [OptionEnum]),
    sub_string(Rust, _, _, _, OptionName),
    sub_string(Ts, _, _, _, OptionEnum),
    sub_string(Rust, _, _, _, '"select_sql"'),
    \+ sub_string(Rust, _, _, _, '"tag":"tag"').

:- end_tests(anonymous_sum_values).
