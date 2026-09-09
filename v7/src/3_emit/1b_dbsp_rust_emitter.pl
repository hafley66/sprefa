:- module(dl7_dbsp_rust_emitter,
          [ render_dbsp_rust/3
          ]).

:- use_module('1a_dbsp_plan_emitter', [emit_dbsp_plan/3]).

%% render_dbsp_rust(+CheckedProgram, -Text, -Diagnostics) is det.
%
% Render the checked plan as direct dd-runner constructors for both storage
% arms. JSON values remain the cell representation, but no serialized program
% is embedded or decoded by the generated module.
render_dbsp_rust(CheckedProgram, Text, Diagnostics) :-
    emit_dbsp_plan(CheckedProgram, Plan, PlanDiagnostics),
    ( PlanDiagnostics == []
    -> with_output_to(string(Text), write_native_plan(Plan)),
       Diagnostics = []
    ;  Text = "",
       Diagnostics = PlanDiagnostics
    ).

write_native_plan(Plan) :-
    writeln('// generated from checked DL7 through the DBSP lowering'),
    writeln('// program structure is native Rust; cells use the kernel Value plane'),
    nl,
    writeln('#[allow(unused_imports)]'),
    writeln('use dd_runner::kernel::{Aggregate, LiteralEquals, Operator, Predicate, Projection};'),
    writeln('use dd_runner::{Rel, Row, Rule};'),
    writeln('#[allow(unused_imports)]'),
    writeln('use std::collections::BTreeMap;'),
    nl,
    write_relations(Plan.rels),
    nl,
    write_ddl(Plan.ddl),
    nl,
    write_rules(Plan.rules),
    nl,
    write_tick_order(Plan.tick_order),
    nl,
    write_initial(Plan.initial),
    nl,
    write_operators(Plan.operators).

write_relations(Relations) :-
    writeln('#[rustfmt::skip]'),
    writeln('pub fn relations() -> Vec<Rel> {'),
    writeln('    vec!['),
    maplist(write_relation, Relations),
    writeln('    ]'),
    writeln('}').

write_relation(Relation) :-
    write('        Rel { name: '),
    write_owned_string(Relation.name),
    write(', columns: vec!['),
    write_owned_strings(Relation.columns),
    write('], select_all: '),
    write_owned_string(Relation.select_all),
    writeln(' },').

write_ddl(Statements) :-
    writeln('#[rustfmt::skip]'),
    writeln('pub fn ddl() -> Vec<String> {'),
    writeln('    vec!['),
    maplist(write_owned_string_line, Statements),
    writeln('    ]'),
    writeln('}').

write_rules(Rules) :-
    writeln('#[rustfmt::skip]'),
    writeln('pub fn rules() -> Vec<Rule> {'),
    writeln('    vec!['),
    maplist(write_rule, Rules),
    writeln('    ]'),
    writeln('}').

write_rule(Rule) :-
    write('        Rule { id: '),
    write_owned_string(Rule.id),
    write(', head: '),
    write_owned_string(Rule.head),
    write(', delete: '),
    write_owned_string(Rule.delete),
    write(', inserts: vec!['),
    write_owned_strings(Rule.inserts),
    writeln('] },').

write_tick_order(Phases) :-
    writeln('#[rustfmt::skip]'),
    writeln('pub fn tick_order() -> Vec<String> {'),
    writeln('    vec!['),
    maplist(write_owned_string_line, Phases),
    writeln('    ]'),
    writeln('}').

write_owned_string_line(Value) :-
    write('        '),
    write_owned_string(Value),
    writeln(',').

write_initial(Rows) :-
    writeln('#[rustfmt::skip]'),
    writeln('pub fn initial() -> Vec<Row> {'),
    writeln('    vec!['),
    maplist(write_row, Rows),
    writeln('    ]'),
    writeln('}').

write_row(Row) :-
    write('        Row { rel: '),
    write_owned_string(Row.rel),
    write(', values: vec!['),
    write_values(Row.values),
    writeln('] },').

write_operators(Operators) :-
    writeln('#[rustfmt::skip]'),
    writeln('pub fn operators() -> Vec<Operator> {'),
    writeln('    vec!['),
    maplist(write_operator, Operators),
    writeln('    ]'),
    writeln('}').

write_operator(Operator) :-
    write('        Operator { id: '),
    write_owned_string(Operator.id),
    write(', kind: '),
    write_owned_string(Operator.kind),
    write(', head: '),
    write_owned_string(Operator.head),
    write(', refs: vec!['),
    write_owned_strings(Operator.refs),
    write('], bindings: BTreeMap::from(['),
    write_bindings(Operator.bindings),
    write(']), predicates: vec!['),
    write_predicates(Operator.predicates),
    write('], projection: vec!['),
    write_projections(Operator.projection),
    write('], aggregate: '),
    write_aggregate(Operator),
    writeln(' },').

write_bindings(Bindings) :-
    dict_pairs(Bindings, _, Pairs),
    write_binding_pairs(Pairs).

write_binding_pairs([]).
write_binding_pairs([Alias-Relation | Pairs]) :-
    write('('),
    write_owned_string(Alias),
    write(', '),
    write_owned_string(Relation),
    write(')'),
    write_separator(Pairs),
    write_binding_pairs(Pairs).

write_predicates([]).
write_predicates([Predicate | Predicates]) :-
    write('Predicate { column_equals: '),
    write_column_equals(Predicate),
    write(', literal_equals: '),
    write_literal_equals(Predicate),
    write(' }'),
    write_separator(Predicates),
    write_predicates(Predicates).

write_column_equals(Predicate) :-
    ( get_dict(column_equals, Predicate, [Left, Right])
    -> write('Some(['),
       write_owned_string(Left),
       write(', '),
       write_owned_string(Right),
       write('])')
    ;  write('None')
    ).

write_literal_equals(Predicate) :-
    ( get_dict(literal_equals, Predicate, Literal)
    -> write('Some(LiteralEquals { column: '),
       write_owned_string(Literal.column),
       write(', value: '),
       write_value(Literal.value),
       write(' })')
    ;  write('None')
    ).

write_projections([]).
write_projections([Projection | Projections]) :-
    write('Projection { head: '),
    write_owned_string(Projection.head),
    write(', source: '),
    write_optional_owned_string(Projection, source),
    write(', value: '),
    write_optional_value(Projection, value),
    write(' }'),
    write_separator(Projections),
    write_projections(Projections).

write_optional_owned_string(Dict, Key) :-
    ( get_dict(Key, Dict, Value)
    -> write('Some('),
       write_owned_string(Value),
       write(')')
    ;  write('None')
    ).

write_optional_value(Dict, Key) :-
    ( get_dict(Key, Dict, Value)
    -> write('Some('),
       write_value(Value),
       write(')')
    ;  write('None')
    ).

write_aggregate(Operator) :-
    ( get_dict(aggregate, Operator, Aggregate)
    -> write('Some(Aggregate { kind: vec!['),
       write_owned_strings(Aggregate.kind),
       write('], group: vec!['),
       write_owned_strings(Aggregate.group),
       write('], value: vec!['),
       write_owned_strings(Aggregate.value),
       write('] })')
    ;  write('None')
    ).

write_owned_strings([]).
write_owned_strings([Value | Values]) :-
    write_owned_string(Value),
    write_separator(Values),
    write_owned_strings(Values).

write_owned_string(Value) :-
    write('String::from('),
    write_rust_string(Value),
    write(')').

write_values([]).
write_values([Value | Values]) :-
    write_value(Value),
    write_separator(Values),
    write_values(Values).

write_value(Value) :-
    integer(Value),
    !,
    format('serde_json::Value::from(~d_i64)', [Value]).
write_value(Value) :-
    float(Value),
    !,
    format('serde_json::Value::from(~16g_f64)', [Value]).
write_value(Value) :-
    ( string(Value) ; atom(Value) ),
    !,
    write('serde_json::Value::String('),
    write_owned_string(Value),
    write(')').
write_value(Values) :-
    is_list(Values),
    !,
    write('serde_json::Value::Array(vec!['),
    write_values(Values),
    write('])').
write_value(Value) :-
    is_dict(Value),
    !,
    dict_pairs(Value, _, Pairs),
    write('serde_json::Value::Object(serde_json::Map::from_iter(['),
    write_value_pairs(Pairs),
    write(']))').

write_value_pairs([]).
write_value_pairs([Key-Value | Pairs]) :-
    write('('),
    write_owned_string(Key),
    write(', '),
    write_value(Value),
    write(')'),
    write_separator(Pairs),
    write_value_pairs(Pairs).

write_separator([]).
write_separator([_ | _]) :-
    write(', ').

write_rust_string(Value) :-
    text_codes(Value, Codes),
    put_code(0'"),
    maplist(write_rust_string_code, Codes),
    put_code(0'").

text_codes(Value, Codes) :-
    string(Value),
    !,
    string_codes(Value, Codes).
text_codes(Value, Codes) :-
    atom_codes(Value, Codes).

write_rust_string_code(0'\\) :- !, write('\\\\').
write_rust_string_code(0'") :- !, write('\\"').
write_rust_string_code(0'\n) :- !, write('\\n').
write_rust_string_code(0'\r) :- !, write('\\r').
write_rust_string_code(0'\t) :- !, write('\\t').
write_rust_string_code(Code) :-
    Code >= 32,
    Code =\= 127,
    !,
    put_code(Code).
write_rust_string_code(Code) :-
    format('\\u{~16r}', [Code]).
