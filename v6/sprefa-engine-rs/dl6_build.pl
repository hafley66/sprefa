% The `.types.rs` emitter shaped as a compile_dl6/3 emitter, so the text door
% reaches the type surface with no second compile pipeline.

:- module(dl6_build, [ emit_types/5 ]).

:- use_module('../prolog/lower', [ catalog_decl_rows/6 ]).
:- use_module('../prolog/compile/4_emit_jsonschema', [ option_rows/3 ]).
:- use_module('../prolog/compile/8_emit_rust_types', [ rust_types_text/3 ]).

emit_types(Name, Plan, _Lowered, _Boot, Text) :-
    Plan = plan(_, prog(Decls, Rules), _, RelPlans, _, _, _, _, _),
    catalog_decl_rows(Name, Rules, RelPlans, Decls, TypeRows, _),
    option_rows(Decls, TypeRows, TypeRowsOpt),
    rust_types_text(Name, TypeRowsOpt, Text).
