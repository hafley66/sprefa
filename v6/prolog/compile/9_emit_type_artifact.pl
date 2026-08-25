:- module(emit_type_artifact,
          [ emit_ts_types/5,
            emit_rust_types/5,
            emit_jsonschema/5
          ]).

:- use_module('../next/2_lower/lower', [ catalog_type_rows/6,
                            catalog_type_relation_rows/3,
                            catalog_type_transport_rows/4 ]).
:- use_module('4_emit_jsonschema', [ jsonschema_text/3, option_rows/3 ]).
:- use_module('7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('8_emit_rust_types', [ rust_types_text/3 ]).

type_rows(Name, Plan, Rows) :-
    Plan = plan(_, prog(Decls, Rules), _, RelPlans, _, _, _, _, Mode),
    catalog_type_rows(Mode, Name, Rules, RelPlans, Decls, Rows0),
    option_rows(Decls, Rows0, RowsOpt),
    catalog_type_relation_rows(Name, Decls, RelationRows),
    catalog_type_transport_rows(Name, RowsOpt, Decls, ChildRows),
    append([RowsOpt, RelationRows, ChildRows], Rows).

emit_ts_types(Name, Plan, _Lowered, _Boot, Text) :-
    type_rows(Name, Plan, Rows),
    ts_types_text(Name, Rows, Text).

emit_rust_types(Name, Plan, _Lowered, _Boot, Text) :-
    type_rows(Name, Plan, Rows),
    rust_types_text(Name, Rows, Text).

emit_jsonschema(Name, Plan, _Lowered, _Boot, Text) :-
    type_rows(Name, Plan, Rows),
    jsonschema_text(Name, Rows, Text).
