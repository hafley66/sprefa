% ═══ the rel record : one term per relation, read by every phase after `plan`
%
% @comment-ok: rel/5's field contract, the record's single documentation site
%   rel(Ref, StorageName, Kind, Cols, KeyOrNone)
%     Ref        Name/Arity.
%     StorageName Physical SQLite base table name.  Ref remains the authored
%                 semantic identity used by rules, arrivals, diagnostics,
%                 and public executable-plan fields.
%     Kind       log | set.
%     Cols       one col/3 per argument position, in position order:
%                  col(Name, declared(WrittenType) | inferred, Storage)
%                declared(WrittenType): a col_type/3 decl names this column,
%                and WrittenType is its SURFACE spelling, so a struct column
%                reads declared(span) while its Storage reads ref(span).
%                inferred: no decl, so Storage came from analyze.pl's
%                literal-witness + head-expression fixpoint, or the rel is
%                compiler-minted (the dictionary tables).
%                Storage: int | float | bool | text | json | json_list(T) |
%                ref(TypeName), the one thing lower.pl:column_def/3 reads.
%     KeyOrNone  key(Positions) | none.
%
% Declared and inferred are two facts of one system and both survive, because
% two consumers need different ones: SQL storage reads Storage at every
% column, while the arrival gate reads the declared slot ONLY -- the reference
% engine's gate is decl-driven, so gating on an inferred type would refuse a
% program engine.pl runs. relplan_declared/2 is that filter, all-or-nothing
% per rel for the same reason 0_type_plane.pl:ref_column_names/4 is: a
% partially typed decl cannot locate its own positions.
%
% type_def/3 (0_type_plane.pl) stays a SIBLING, not a fourth slot: its
% consumers run BEFORE any rel record exists, because column_storage/3 needs
% the type table to resolve a struct name into ref(Name) and that call is what
% FILLS this record's Storage slot. Every struct type still reaches this record
% as an ordinary rel, via compile.pl:materialize_reference_target_rels/2.
%
% The relplan_ prefix names the LIST plan/9 carries, not a second record; the
% bare rel_ prefix is taken by analyze.pl (rel_kind/3, rel_columns/4).

:- module(rel_record,
          [ rel_cols/4,
            inferred_cols/3,
            relplan_parts/6,
            relplan_storage_name/2,
            relplan_storage_name/3,
            relplan_origins/2,
            relplan_declared/2,
            relplan_of/3,
            relplan_shape/6,
            relplan_kind/3,
            relplan_columns/3,
            relplan_column_types/3,
            relplan_key/3,
            relplan_declared_types/3,
            relplan_reference_target/2,
            relplan_reference_targets/2
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

% ═══ one record ══════════════════════════════════════════════════════════════

% Bidirectional: any one proper list drives the recursion.
rel_cols([], [], [], []).
rel_cols([Name | Names], [Origin | Origins], [Storage | Storages],
         [col(Name, Origin, Storage) | Cols]) :-
    rel_cols(Names, Origins, Storages, Cols).

inferred_cols(Names, Storages, Cols) :-
    maplist(inferred_col, Names, Storages, Cols).

inferred_col(Name, Storage, col(Name, inferred, Storage)).

% Destructure. Construction writes the rel/5 term out beside rel_cols/4 or
% inferred_cols/3, so no Origin slot is ever left a fresh variable.
% rel/4 remains accepted for hand-built unit plans and compiler-minted
% dictionary plans.  Its physical name defaults to the semantic relation name.
relplan_parts(rel(Ref, _StorageName, Kind, Cols, KeyOrNone), Ref, Kind, Columns, KeyOrNone,
              ColumnTypes) :-
    rel_cols(Columns, _Origins, ColumnTypes, Cols).
relplan_parts(rel(Ref, Kind, Cols, KeyOrNone), Ref, Kind, Columns, KeyOrNone,
              ColumnTypes) :-
    rel_cols(Columns, _Origins, ColumnTypes, Cols).

relplan_storage_name(rel(_Ref, StorageName, _Kind, _Cols, _KeyOrNone), StorageName) :- !.
relplan_storage_name(rel(Ref, _Kind, _Cols, _KeyOrNone), StorageName) :-
    Ref = StorageName/_.

relplan_storage_name(RelPlans, Ref, StorageName) :-
    relplan_of(RelPlans, Ref, RelPlan),
    relplan_storage_name(RelPlan, StorageName).

relplan_origins(rel(_, _, _, Cols, _), Origins) :- !,
    rel_cols(_Columns, Origins, _ColumnTypes, Cols).
relplan_origins(rel(_, _, Cols, _), Origins) :-
    rel_cols(_Columns, Origins, _ColumnTypes, Cols).

relplan_declared(rel(_, _, _, Cols, _), DeclaredTypes) :- !,
    maplist(declared_col_type, Cols, DeclaredTypes).
relplan_declared(rel(_, _, Cols, _), DeclaredTypes) :-
    maplist(declared_col_type, Cols, DeclaredTypes).

declared_col_type(col(_, declared(Type), _), Type).

% ═══ lookup in plan/9's RelPlans ═════════════════════════════════════════════

relplan_of(RelPlans, Ref, Rel) :-
    member(Rel, RelPlans),
    relplan_parts(Rel, Ref, _, _, _, _),
    !.

relplan_shape(RelPlans, Ref, Kind, Columns, KeyOrNone, ColumnTypes) :-
    relplan_of(RelPlans, Ref, Rel),
    relplan_parts(Rel, Ref, Kind, Columns, KeyOrNone, ColumnTypes).

relplan_kind(RelPlans, Ref, Kind) :- relplan_shape(RelPlans, Ref, Kind, _, _, _).

relplan_columns(RelPlans, Ref, Columns) :-
    relplan_shape(RelPlans, Ref, _, Columns, _, _).

relplan_column_types(RelPlans, Ref, ColumnTypes) :-
    relplan_shape(RelPlans, Ref, _, _, _, ColumnTypes).

relplan_key(RelPlans, Ref, KeyOrNone) :-
    relplan_shape(RelPlans, Ref, _, _, KeyOrNone, _).

relplan_declared_types(RelPlans, Ref, DeclaredTypes) :-
    relplan_of(RelPlans, Ref, Rel),
    relplan_declared(Rel, DeclaredTypes).

% ═══ relation identity targets ═════════════════════════════════════════════

% The set of type names a plan's rels point at through a ref(TypeName) column
% storage. One TargetName per distinct occurrence (a column may repeat it).
relplan_reference_target(RelPlans, TargetName) :-
    member(Rel, RelPlans),
    relplan_parts(Rel, _, _, _, _, ColumnTypes),
    member(ref(TargetName), ColumnTypes).

relplan_reference_targets(RelPlans, TargetNames) :-
    findall(TargetName,
            relplan_reference_target(RelPlans, TargetName),
            AllTargetNames),
    sort(AllTargetNames, TargetNames).
