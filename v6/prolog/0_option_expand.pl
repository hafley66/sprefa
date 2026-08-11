% option(T) decl sugar (plans/2026-08-08-option-type-design.md, ruling
% option_surface): scalar -> '__opt_<t>' enum id, rel ref -> companion split rel.
:- module(option_expand,
          [ expand_option_in_context/3,
            expand_option_program/2,
            expand_option_decls/2,
            option_enum_name/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

expand_option_in_context(_Context, Program, Expanded) :-
    expand_option_program(Program, Expanded).

% Rules untouched: authors write bodies against the desugared rels, the
% same consumption shape enums already have.
expand_option_program(prog(Decls0, Rules), prog(Decls, Rules)) :-
    expand_option_decls(Decls0, Decls).

expand_option_decls(Decls0, Decls) :-
    ( member(col_type(Ref, Column, option(Element)), Decls0)
    -> desugar_option_column(Decls0, Ref, Column, Element, Decls1),
       expand_option_decls(Decls1, Decls)
    ; Decls = Decls0
    ).

desugar_option_column(Decls0, Ref, Column, Element, Decls) :-
    ( option_column_position(Decls0, Ref, Column, Position)
    -> true
    ; throw(unsupported_construct(option_column_untyped_siblings(Ref)))
    ),
    ( memberchk(keyed(Ref, KeyPositions), Decls0),
      memberchk(Position, KeyPositions)
    -> throw(unsupported_construct(option_in_key_column(Ref, Column)))
    ; true
    ),
    % option_column/3 survives so the schema emitters can fold the desugared
    % column back to a nullable anyOf.
    ( scalar_element(Element)
    -> desugar_scalar_option(Decls0, Ref, Column, Element, Decls1),
       append(Decls1, [option_column(Ref, Column, Element)], Decls)
    ; memberchk(enum_decl(Element, _), Decls0)
    -> throw(unsupported_construct(option_of_enum_unsupported(Element)))
    ; declared_rel_element(Decls0, Element)
    -> desugar_reference_option(Decls0, Ref, Column, Element, Position,
                                Decls1),
       append(Decls1, [option_column(Ref, Column, Element)], Decls)
    ; throw(unsupported_construct(option_element_type_unknown(Element)))
    ).

scalar_element(int).
scalar_element(text).
scalar_element(bool).
scalar_element(float).
scalar_element(json).

declared_rel_element(Decls, Element) :-
    atom(Element),
    memberchk(col_type(Element/_, _, _), Decls).

% Positions are meaningful only fully typed; the length check is that
% requirement.
option_column_position(Decls, Name/Arity, Column, Position) :-
    findall(EachColumn,
            member(col_type(Name/Arity, EachColumn, _), Decls),
            Columns),
    length(Columns, Arity),
    nth1(Position, Columns, Column).

desugar_scalar_option(Decls0, Ref, Column, Element, Decls) :-
    option_enum_name(Element, EnumName),
    selectchk(col_type(Ref, Column, option(Element)),
              Decls0,
              col_type(Ref, Column, EnumName),
              Decls1),
    ( memberchk(enum_decl(EnumName, _), Decls1)
    -> Decls = Decls1
    ; Decls = [enum_decl(EnumName, (none ; some(value:Element))) | Decls1]
    ).

option_enum_name(Element, EnumName) :-
    atomic_list_concat(['__opt_', Element], EnumName).

desugar_reference_option(Decls0, ParentName/Arity, Column, Element, Position,
                         Decls) :-
    NewArity is Arity - 1,
    exclude(option_column_entry(ParentName/Arity, Column), Decls0, Kept),
    maplist(shrink_parent_ref(ParentName/Arity, ParentName/NewArity,
                              Position),
            Kept, RewrittenDecls),
    companion_rel_decls(ParentName, Column, Element, CompanionDecls),
    append(RewrittenDecls, CompanionDecls, Decls).

option_column_entry(Ref, Column, col_type(Ref, Column, option(_))).

shrink_parent_ref(OldRef, NewRef, _, col_type(OldRef, Column, Type),
                  col_type(NewRef, Column, Type)) :- !.
shrink_parent_ref(OldRef, NewRef, DroppedPosition,
                  keyed(OldRef, Positions), keyed(NewRef, Renumbered)) :-
    !,
    maplist(renumber_key_position(DroppedPosition), Positions, Renumbered).
shrink_parent_ref(OldRef, NewRef, _, kind(OldRef, Kind),
                  kind(NewRef, Kind)) :- !.
shrink_parent_ref(OldRef, NewRef, _, keep(OldRef, Policy),
                  keep(NewRef, Policy)) :- !.
% The path carrier is keyed on the ref, so a shrink that skips it leaves the
% dot phase reading an arity no other decl carries.
shrink_parent_ref(OldRef, NewRef, _, rel_path_decl(OldRef, Segments),
                  rel_path_decl(NewRef, Segments)) :- !.
shrink_parent_ref(_, _, _, Decl, Decl).

% DroppedPosition itself cannot appear: the key-column ban threw first.
renumber_key_position(DroppedPosition, Position, Renumbered) :-
    ( Position > DroppedPosition
    -> Renumbered is Position - 1
    ; Renumbered = Position
    ).

companion_rel_decls(ParentName, Column, Element,
                    [ col_type(CompanionRef, ParentIdColumn, int),
                      col_type(CompanionRef, ElementIdColumn, int),
                      keyed(CompanionRef, [1]) ]) :-
    atomic_list_concat([ParentName, '__', Column], CompanionName),
    CompanionRef = CompanionName/2,
    atom_concat(ParentName, '_id', ParentIdColumn),
    atom_concat(Element, '_id', ElementIdColumn).
