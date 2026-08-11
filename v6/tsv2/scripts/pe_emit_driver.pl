% pe_emit_driver.pl — emit 4_emit_jsonschema / 5_emit_openapi for one .dl6
% program; a refusal propagates as the round-trip gate's serialization gap.
:- module(pe_emit_driver, [main/2]).

:- use_module('./compile.pl', [default_intern_mode/1, dl6_seeded_form/3, program_plan/3]).
:- use_module('./lower', [catalog_decl_rows/6]).
:- use_module('./compile/4_emit_jsonschema', [jsonschema_text/3]).
:- use_module('./compile/5_emit_openapi', [openapi_text/3]).
:- use_module('./use_resolve', [expand_uses/8]).
:- use_module(library(filesex)).

main(File, OutDir) :-
    make_directory_path(OutDir),
    default_intern_mode(Mode),
    expand_uses(File, [], [], _, Prog, _, Bindings, Findings),
    (   Findings == []
    ->  true
    ;   format(user_error, 'emit-back: parse findings ~q~n', [Findings]),
        fail
    ),
    dl6_seeded_form(Prog, Initial, ProgOut),
    file_base_name(File, BaseName),
    file_name_extension(Name, _Ext, BaseName),
    program_plan(fixture(Name, ProgOut, Initial, [], [])-Bindings,
                 [intern(Mode)], Plan),
    Plan = plan(_, prog(Decls, Rules), _, RelPlans, _, _, _, _, _),
    catalog_decl_rows(Name, Rules, RelPlans, Decls, Rows, _),
    atomic_list_concat([OutDir, '/schema.json'], SchemaPath),
    atomic_list_concat([OutDir, '/openapi.json'], OpenapiPath),
    emit_back(Name, Rows, jsonschema_text, SchemaPath),
    emit_back(Name, Rows, openapi_text, OpenapiPath).

emit_back(Name, Rows, Emitter, Path) :-
    (   call(Emitter, Name, Rows, Text)
    ->  setup_call_cleanup(open(Path, write, S), format(S, '~s', [Text]), close(S)),
        format('emit-back wrote ~w~n', [Path])
    ;   format(user_error, 'emit-back REFUSED: emitter ~q on ~w~n', [Emitter, Name]),
        fail
    ).
