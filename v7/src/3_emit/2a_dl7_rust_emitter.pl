:- module(dl7_rust_emitter,
          [ render_dl7_rust_file/3,
            render_dl7_rust_program/4
          ]).

:- use_module('../2_comptime/2_compiler', [compile_dl7/4]).

%% render_dl7_rust_file(+Path, -Text, -Diagnostics) is det.
render_dl7_rust_file(Path, Text, Diagnostics) :-
    compile_dl7(Path, _, Runtime, CompileDiagnostics),
    ( CompileDiagnostics == []
    -> once(absolute_file_name(Path, AbsolutePath,
                               [access(read), file_errors(error)])),
       render_dl7_rust_program(
           AbsolutePath, Runtime, Text, Diagnostics)
    ;  Text = "",
       Diagnostics = CompileDiagnostics
    ).

%% render_dl7_rust_program(+Path, +CheckedProgram, -Text, -Diagnostics) is det.
render_dl7_rust_program(
    Path,
    checked_datalog(root_graph(Nodes, Edges), _, _, _),
    Text, Diagnostics) :-
    authored_types(Path, Nodes, Edges, Types),
    type_diagnostics(Types, Edges, Diagnostics),
    ( Diagnostics == []
    -> with_output_to(string(Text), write_types(Path, Types, Edges))
    ;  Text = ""
    ).

authored_types(Path, Nodes, Edges, Types) :-
    findall(
        Index-type(Name, Identity, Kind),
        ( member(':'(module(file(Path)), Name, ref(Identity), Index), Edges),
          Identity = owner(file(Path), _),
          node_kind(Nodes, Identity, Kind)
        ),
        Indexed0),
    sort(Indexed0, Indexed),
    pairs_values(Indexed, Types).

node_kind(Nodes, Identity, product) :-
    memberchk(product(Identity), Nodes),
    !.
node_kind(Nodes, Identity, sum) :-
    memberchk(sum(Identity), Nodes).

type_diagnostics(Types, Edges, Diagnostics) :-
    findall(Diagnostic,
            type_diagnostic(Types, Edges, Diagnostic),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics).

type_diagnostic(Types, _Edges,
                diagnostic(emit, none,
                           invalid_rust_type_name(Name))) :-
    member(type(Name, _, _), Types),
    \+ rust_identifier(Name, _).
type_diagnostic(Types, Edges,
                diagnostic(emit, none,
                           invalid_rust_field_name(TypeName, Label))) :-
    member(type(TypeName, Identity, _), Types),
    member(':'(Identity, Label, _, _), Edges),
    \+ rust_identifier(Label, _).
type_diagnostic(Types, Edges,
                diagnostic(emit, none,
                           unknown_rust_field_type(TypeName, Label, Target))) :-
    member(type(TypeName, Identity, _), Types),
    member(':'(Identity, Label, ref(Target), _), Edges),
    \+ rust_target_type(Target, Types, Edges, _).

write_types(Path, Types, Edges) :-
    display_path(Path, DisplayPath),
    format('// generated from ~w~n', [DisplayPath]),
    writeln('// source of truth: DL7 product and sum edges'),
    nl,
    write_type_list(Types, Types, Edges).

display_path(Path, DisplayPath) :-
    working_directory(Directory, Directory),
    atom_concat(Directory, Relative, Path),
    !,
    DisplayPath = Relative.
display_path(Path, Path).

write_type_list([], _, _).
write_type_list([Type | Rest], Types, Edges) :-
    write_type(Type, Types, Edges),
    ( Rest == [] -> true ; nl ),
    write_type_list(Rest, Types, Edges).

write_type(type(Name, Identity, product), Types, Edges) :-
    rust_identifier(Name, RustName),
    writeln('#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]'),
    format('pub struct ~w {~n', [RustName]),
    type_edges(Identity, Edges, Fields),
    write_struct_fields(Fields, Types, Edges),
    writeln('}').
write_type(type(Name, Identity, sum), Types, Edges) :-
    rust_identifier(Name, RustName),
    writeln('#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]'),
    format('pub enum ~w {~n', [RustName]),
    type_edges(Identity, Edges, Variants),
    write_enum_variants(Variants, Types, Edges),
    writeln('}').

type_edges(Identity, Edges, Ordered) :-
    findall(Index-edge(Label, Target),
            member(':'(Identity, Label, ref(Target), Index), Edges),
            Indexed0),
    sort(Indexed0, Indexed),
    pairs_values(Indexed, Ordered).

write_struct_fields([], _, _).
write_struct_fields([edge(Label, Target) | Fields], Types, Edges) :-
    rust_identifier(Label, RustLabel0),
    rust_field_name(RustLabel0, RustLabel),
    rust_target_type(Target, Types, Edges, RustType),
    format('    pub ~w: ~w,~n', [RustLabel, RustType]),
    write_struct_fields(Fields, Types, Edges).

write_enum_variants([], _, _).
write_enum_variants([edge(Label, Target) | Variants], Types, Edges) :-
    rust_identifier(Label, RustVariant),
    rust_target_type(Target, Types, Edges, RustType),
    ( RustType == '()'
    -> format('    ~w,~n', [RustVariant])
    ;  format('    ~w(~w),~n', [RustVariant, RustType])
    ),
    write_enum_variants(Variants, Types, Edges).

rust_target_type(primitive(text), _, _, 'String').
rust_target_type(primitive(int), _, _, 'i64').
rust_target_type(primitive(any), _, _, 'serde_json::Value').
rust_target_type(primitive(type), _, _, 'String').
rust_target_type(Target, Types, _, RustType) :-
    memberchk(type(Name, Target, _), Types),
    rust_identifier(Name, RustType),
    !.
rust_target_type(Target, _, Edges, RustType) :-
    member(':'(module(prelude), Name, ref(Target), _), Edges),
    rust_prelude_type(Name, RustType),
    !.

rust_prelude_type('()', '()').
rust_prelude_type(bool, bool).
rust_prelude_type(char, char).
rust_prelude_type(str, 'String').
rust_prelude_type(string, 'String').
rust_prelude_type(number, f64).
rust_prelude_type(boolean, bool).
rust_prelude_type(Name, Name) :-
    memberchk(Name, [i8,i16,i32,i64,i128,u8,u16,u32,u64,u128,
                     f32,f64,usize,isize]).

rust_identifier(Name, Identifier) :-
    atom(Name),
    atom_codes(Name, Codes),
    Codes = [First | Rest],
    rust_identifier_first(First),
    maplist(rust_identifier_rest, Rest),
    Identifier = Name.

rust_identifier_first(Code) :-
    code_type(Code, alpha),
    !.
rust_identifier_first(0'_).

rust_identifier_rest(Code) :-
    code_type(Code, alnum),
    !.
rust_identifier_rest(0'_).

rust_field_name(Name, Escaped) :-
    ( rust_keyword(Name)
    -> format(atom(Escaped), 'r#~w', [Name])
    ;  Escaped = Name
    ).

rust_keyword(type).
rust_keyword(match).
rust_keyword(ref).
rust_keyword(self).
rust_keyword('Self').
rust_keyword(super).
rust_keyword(crate).
rust_keyword(move).
rust_keyword(async).
rust_keyword(await).
rust_keyword(loop).
rust_keyword(where).
