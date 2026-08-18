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

:- end_tests(emit_type_renderers).
