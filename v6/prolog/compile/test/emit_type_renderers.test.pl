:- begin_tests(emit_type_renderers).

:- use_module('../7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('../8_emit_rust_types', [ rust_types_text/3 ]).

type_rows([
    row(1, 0, 0, int, primitive, 0, 0, 0, '', '', ''),
    row(2, 0, 0, float, primitive, 0, 0, 0, '', '', ''),
    row(3, 0, 0, text, primitive, 0, 0, 0, '', '', ''),
    row(4, 0, 0, bool, primitive, 0, 0, 0, '', '', ''),
    row(5, 0, 0, json, primitive, 0, 0, 0, '', '', ''),
    row(6, 0, 0, 'json_list(int)', json_list, 1, 0, 0, '', '', ''),
    row(7, 0, 0, 'option(text)', option, 3, 0, 0, '', '', ''),
    row(8, 0, 0, child, rel, 0, 1, 1, '', '', ''),
    row(9, 0, 0, parent, rel, 0, 6, 1, '', '', ''),
    row(10, 9, 0, count, column, 1, 0, 1, '', '', ''),
    row(11, 9, 1, ratio, column, 2, 0, 1, '', '', ''),
    row(12, 9, 2, name, column, 3, 0, 1, '', '', ''),
    row(13, 9, 3, active, column, 4, 0, 1, '', '', ''),
    row(14, 9, 4, value, column, 5, 0, 1, '', '', ''),
    row(15, 9, 5, values, column, 6, 0, 1, '', '', ''),
    row(16, 9, 6, note, column, 7, 0, 1, '', '', ''),
    row(17, 9, 7, child, column, 8, 0, 1, '', '', ''),
    row(18, 8, 0, id, column, 1, 0, 1, '', '', ''),
    row(19, 0, 0, 'list(int)', list, 1, 0, 0, '', '', ''),
    row(20, 0, 0, holder, rel, 0, 1, 1, '', '', ''),
    row(21, 20, 0, values, column, 19, 0, 1, '', '', '')
]).

test(ts_types) :-
    type_rows(Rows),
    once(ts_types_text(main, Rows, Text)),
    Text == "export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };\n\nexport interface Child {\n  id: number;\n}\n\nexport interface Parent {\n  count: number;\n  ratio: number;\n  name: string;\n  active: boolean;\n  value: unknown;\n  values: Array<number>;\n  note: Option<string>;\n  child: Child;\n}\n\nexport interface Holder {\n  values: Array<number>;\n}\n".

test(rust_types) :-
    type_rows(Rows),
    once(rust_types_text(main, Rows, Text)),
    Text == "#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\n#[serde(tag = \"tag\", content = \"value\", rename_all = \"snake_case\")]\npub enum DlOption<T> {\n    None,\n    Some(T),\n}\n\n#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct Child {\n    pub id: i64,\n}\n\n#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct Parent {\n    pub count: i64,\n    pub ratio: f64,\n    pub name: String,\n    pub active: bool,\n    pub value: serde_json::Value,\n    pub values: Vec<i64>,\n    pub note: DlOption<String>,\n    pub child: Child,\n}\n\n#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct Holder {\n    pub values: Vec<i64>,\n}\n".

generic_rows([
    row(1, 0, 0, int, primitive, 0, 0, 0, '', '', ''),
    row(6, 0, 0, json_encodable, interface, 0, 0, 0, '', '', ''),
    row(7, 0, 0, pair, generic_rel, 0, 0, 0, '', '', ''),
    row(8, 7, 1, 'T', type_parameter, 0, 0, 0, '', '', ''),
    row(9, 8, 1, json_encodable, constraint, 6, 0, 0, '', '', ''),
    row(10, 7, 1, first, generic_column, 8, 0, 0, '', '', ''),
    row(11, 7, 2, second, generic_column, 8, 0, 0, '', '', '')
]).

test(ts_preserves_generic_declaration_and_bound) :-
    generic_rows(Rows),
    once(ts_types_text(main, Rows, Text)),
    Text == "export interface JsonEncodable {}\n\nexport interface Pair<T extends JsonEncodable> {\n  first: T;\n  second: T;\n}\n".

test(rust_preserves_generic_declaration_and_bound) :-
    generic_rows(Rows),
    once(rust_types_text(main, Rows, Text)),
    Text == "pub trait JsonEncodable {}\n\n#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct Pair<T: JsonEncodable> {\n    pub first: T,\n    pub second: T,\n}\n".

% A concrete generic sum (minted by generic expansion, lowered by enum
% expansion) renders as a tagged union carrying its substituted payload types.
% These rows are what the catalog produces for
%   Result(host_error, boop_response)
%   with payload relations host_error(code:int) and boop_response(body:text).
generic_sum_rows([
    row(1, 0, 0, int, primitive, 0, 0, 0, '', '', ''),
    row(2, 0, 0, text, primitive, 0, 0, 0, '', '', ''),
    row(10, 0, 0, host_error, rel, 0, 0, 10, '', '', ''),
    row(11, 10, 1, code, column, 1, 0, 10, '', '', ''),
    row(20, 0, 0, boop_response, rel, 0, 0, 10, '', '', ''),
    row(21, 20, 1, body, column, 2, 0, 10, '', '', ''),
    row(30, 0, 0, result_hm, rel, 0, 0, 10, '', '', ''),
    row(31, 30, 1, id, column, 1, 0, 10, '', '', ''),
    row(32, 30, 2, outcome, column, 40, 0, 10, '', '', ''),
    row(40, 0, 0, concrete_result, enum, 0, 0, 0, '', '', ''),
    row(41, 40, 1, err, enum_variant, 50, 0, 0, '', '', ''),
    row(42, 40, 2, ok, enum_variant, 60, 0, 0, '', '', ''),
    row(50, 0, 0, concrete_result_err, rel, 0, 0, 10, '', '', ''),
    row(51, 50, 1, id, column, 1, 0, 10, '', '', ''),
    row(52, 50, 2, error, column, 10, 0, 10, '', '', ''),
    row(60, 0, 0, concrete_result_ok, rel, 0, 0, 10, '', '', ''),
    row(61, 60, 1, id, column, 1, 0, 10, '', '', ''),
    row(62, 60, 2, value, column, 20, 0, 10, '', '', '')
]).

test(ts_concrete_sum_emits_a_tagged_union_with_substituted_payloads) :-
    generic_sum_rows(Rows),
    once(ts_types_text(main, Rows, Text)),
    sub_atom(Text, _, _, _, "export type ConcreteResult ="),
    sub_atom(Text, _, _, _, "{ tag: 'err'; error: HostError; }"),
    sub_atom(Text, _, _, _, "{ tag: 'ok'; value: BoopResponse; }"),
    sub_atom(Text, _, _, _, "outcome: ConcreteResult;"), !.

test(rust_concrete_sum_emits_a_serde_tagged_enum_with_substituted_payloads) :-
    generic_sum_rows(Rows),
    once(rust_types_text(main, Rows, Text)),
    sub_atom(Text, _, _, _, "#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub enum ConcreteResult {"),
    sub_atom(Text, _, _, _, "Err { error: HostError },"),
    sub_atom(Text, _, _, _, "Ok { value: BoopResponse },"),
    sub_atom(Text, _, _, _, "pub outcome: ConcreteResult,"), !.


:- end_tests(emit_type_renderers).
