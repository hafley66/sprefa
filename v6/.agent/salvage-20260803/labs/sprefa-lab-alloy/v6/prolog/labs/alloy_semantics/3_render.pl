% 3_render.pl : term-tree emitters for ts and rust.
%
% NO string concat mid-tree: build a full term tree first, then fold to text
% at the end. The ts emitter yields ts_file(FileId, Imports, Interfaces) and
% the rust emitter yields rust_mod(FileId, Uses, Structs); the fold happens in
% tree_to_text/3 only.
%
% The import/use lines are DERIVED from import_needed/4 (which itself is
% derived from ref/2), never hand-written: a cross-file ref becomes an import
% in exactly the file that contains the reference.

:- module(alloy_render,
          [ run_all/0,
            target_text/2,
            ts_text/1,
            rust_text/1
          ]).

:- use_module('0_facts').
:- use_module('1_collect').
:- use_module('2_check').

run_all :-
    collect(ts),
    collect(rust),
    check_target(ts),
    check_target(rust),
    target_text(ts, TS),
    target_text(rust, Rust),
    format('~s~n~n---~n~n~s~n', [TS, Rust]).

% ---- top level: one tree per target file -------------------------------------

target_text(Target, Text) :-
    findall(File, decl(Target, _, _, File), FilesDup),
    sort(FilesDup, Files),
    maplist(file_term(Target), Files, FileTerms),
    maplist(tree_to_text(Target), FileTerms, FileTexts),
    atomic_list_concat(FileTexts, '\n\n', Text).

file_term(Target, File, Tree) :-
    findall(Id, decl(Target, Id, _, File), Ids),
    maplist(decl_term(Target), Ids, DeclTerms),
    findall(Imp,
            ( import_needed(Target, File, ToFile, Id),
              decl(Target, Id, _, ToFile),
              rendered_name(Target, Id, Name),
              import_term(Target, ToFile, Name, Imp) ),
            ImpTerms),
    ( Target == ts
    -> Tree = ts_file(File, ImpTerms, DeclTerms)
    ;  Tree = rust_mod(File, ImpTerms, DeclTerms) ).

decl_term(Target, Id, DTerm) :-
    decl_table(Id, Table),
    findall(Field,
            ( column(Table, Col, Type, Null, _, _, _, _),
              field_term(Target, Col, Type, Null, Field) ),
            Fields),
    rendered_name(Target, Id, Name),
    ( Target == ts
    -> DTerm = ts_interface(Name, Fields)
    ;  DTerm = rust_struct(Name, Fields) ).

% ---- field terms (scalar types; the ref is carried by import, not by type) ----

field_term(ts, Col, Type, Null, ts_field(Col, TsType, Null)) :-
    ts_scalar(Type, TsType).
field_term(rust, Col, Type, Null, rust_field(Col, RsType)) :-
    rust_scalar(Type, Null, RsType).

ts_scalar(i64, number).
ts_scalar(i32, number).
ts_scalar(string, string).

rust_scalar(i64,    false, 'i64').
rust_scalar(i64,    true,  'Option<i64>').
rust_scalar(i32,    false, 'i32').
rust_scalar(string, false, 'String').

% ---- import/use terms, derived, target-specific spellings --------------------

import_term(ts, ToFile, Name, ts_import(Name, Path)) :-
    ts_import_path(ToFile, Path).
import_term(rust, ToFile, Name, rust_use(Path)) :-
    rust_use_path(ToFile, Name, Path).

ts_import_path(ToFile, Path) :-
    atom_concat(Base, '.ts', ToFile),
    atomic_list_concat(['./', Base], Path).
rust_use_path(RustBase, Name, Path) :-
    atomic_list_concat(['super::', RustBase, '::', Name], Path).

% ---- the fold: term tree -> text ---------------------------------------------

tree_to_text(ts, ts_file(_, Imports, Interfaces), Text) :-
    maplist(ts_import_line, Imports, ImportLines),
    maplist(ts_interface_text, Interfaces, InterfaceTexts),
    join_text(ImportLines, InterfaceTexts, Text).

tree_to_text(rust, rust_mod(_, Uses, Structs), Text) :-
    maplist(rust_use_line, Uses, UseLines),
    maplist(rust_struct_text, Structs, StructTexts),
    join_text(UseLines, StructTexts, Text).

join_text([], Bodies, Text) :-
    atomic_list_concat(Bodies, '\n\n', Text).
join_text(Heads, Bodies, Text) :-
    atomic_list_concat(Heads, '\n', HeadsText),
    atomic_list_concat(Bodies, '\n\n', BodiesText),
    format(string(Text), '~w~n~n~w', [HeadsText, BodiesText]).

ts_import_line(ts_import(Name, Path), Line) :-
    format(string(Line), 'import type { ~w } from "~w";', [Name, Path]).

ts_interface_text(ts_interface(Name, Fields), Text) :-
    maplist(ts_field_line, Fields, FieldLines),
    atomic_list_concat(FieldLines, '\n', FieldsText),
    format(string(Text), 'export interface ~w {~n~w~n}', [Name, FieldsText]).

ts_field_line(ts_field(Name, Type, true),  Line) :-
    format(string(Line), '  ~w: ~w | null;', [Name, Type]).
ts_field_line(ts_field(Name, Type, false), Line) :-
    format(string(Line), '  ~w: ~w;', [Name, Type]).

rust_use_line(rust_use(Path), Line) :-
    format(string(Line), 'use ~w;', [Path]).

rust_struct_text(rust_struct(Name, Fields), Text) :-
    maplist(rust_field_line, Fields, FieldLines),
    atomic_list_concat(FieldLines, ',\n', FieldsText),
    format(string(Text), 'pub struct ~w {~n~w,~n}', [Name, FieldsText]).

rust_field_line(rust_field(Name, Type), Line) :-
    format(string(Line), '  pub ~w: ~w', [Name, Type]).

% ---- per-target text helpers (receipt legibility) ----------------------------

ts_text(Text)   :- collect(ts),   target_text(ts, Text).
rust_text(Text) :- collect(rust), target_text(rust, Text).
