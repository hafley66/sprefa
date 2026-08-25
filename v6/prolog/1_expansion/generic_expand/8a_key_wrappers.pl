%! normalize_key_wrappers(+Decls0, -Decls) is det.
% Normalize key(T) into its value type and the existing keyed/2 declaration.
normalize_key_wrappers(Decls0, Decls) :-
    validate_key_wrapper_types(Decls0),
    findall(Ref-Position,
            key_wrapper_position(Decls0, Ref, _Column, Position),
            WrapperPairs0),
    (   WrapperPairs0 == []
    ->  Decls = Decls0
    ;   sort(WrapperPairs0, WrapperPairs),
        reject_repeated_key_wrapper_positions(WrapperPairs0),
        wrapper_key_positions(WrapperPairs, WrapperKeys),
        reconcile_wrapper_and_legacy_keys(Decls0, WrapperKeys,
                                          CanonicalKeys),
        rewrite_key_declarations(Decls0, CanonicalKeys, Decls)
    ).

% Only an outer column wrapper is accepted; option(T) reaches option expansion.
validate_key_wrapper_types(Decls) :-
    forall(member(col_type(Ref, Column, Type), Decls),
           validate_key_wrapper_type(Decls, Ref, Column, Type)).

validate_key_wrapper_type(Decls, Ref, Column, key(Inner)) :-
    !,
    key_wrapper_column_position(Decls, Ref, Column, Position),
    (   Inner = key(_)
    ->  throw(unsupported_construct(key_wrapper_repeated(Ref, Position)))
    ;   contains_key_wrapper(Inner)
    ->  throw(unsupported_construct(key_wrapper_nested(Ref, Position)))
    ;   true
    ).
validate_key_wrapper_type(Decls, Ref, Column, Type) :-
    (   contains_key_wrapper(Type)
    ->  key_wrapper_column_position(Decls, Ref, Column, Position),
        throw(unsupported_construct(key_wrapper_nested(Ref, Position)))
    ;   true
    ).

contains_key_wrapper(Type) :-
    compound(Type),
    Type =.. [key | _],
    !.
contains_key_wrapper(Type) :-
    compound(Type),
    compound_name_arguments(Type, _, Arguments),
    member(Argument, Arguments),
    contains_key_wrapper(Argument),
    !.

key_wrapper_position(Decls, Ref, Column, Position) :-
    member(col_type(Ref, Column, key(_)), Decls),
    key_wrapper_column_position(Decls, Ref, Column, Position).

key_wrapper_column_position(Decls, Ref, Column, Position) :-
    findall(EachColumn,
            member(col_type(Ref, EachColumn, _), Decls), Columns),
    length(Columns, _Arity),
    nth1(Position, Columns, Column),
    !.

reject_repeated_key_wrapper_positions(Pairs) :-
    member(Ref-Position, Pairs),
    select(Ref-Position, Pairs, Rest),
    memberchk(Ref-Position, Rest),
    !,
    throw(unsupported_construct(key_wrapper_repeated(Ref, Position))).
reject_repeated_key_wrapper_positions(_).

wrapper_key_positions(Pairs, Keys) :-
    findall(Ref, member(Ref-_, Pairs), Refs0),
    sort(Refs0, Refs),
    maplist(wrapper_key_positions_for_ref(Pairs), Refs, Keys).

wrapper_key_positions_for_ref(Pairs, Ref, Ref-Positions) :-
    findall(Position, member(Ref-Position, Pairs), Positions0),
    sort(Positions0, Positions).

% Legacy positions share this canonical relation-column-order representation.
reconcile_wrapper_and_legacy_keys(Decls, WrapperKeys, CanonicalKeys) :-
    maplist(reconcile_one_wrapper_key(Decls), WrapperKeys, CanonicalKeys).

reconcile_one_wrapper_key(Decls, Ref-WrapperPositions,
                          Ref-CanonicalPositions) :-
    findall(Legacy, member(keyed(Ref, Legacy), Decls), LegacyLists),
    (   LegacyLists == []
    ->  CanonicalPositions = WrapperPositions
    ;   maplist(sort, LegacyLists, SortedLegacyLists),
        (   member(LegacyPositions, SortedLegacyLists),
            LegacyPositions \== WrapperPositions
        ->  throw(unsupported_construct(
                      key_wrapper_legacy_conflict(Ref, WrapperPositions,
                                                   LegacyPositions)))
        ;   CanonicalPositions = WrapperPositions
        )
    ).

rewrite_key_declarations(Decls0, CanonicalKeys, Decls) :-
    rewrite_key_declarations(Decls0, CanonicalKeys, [], Decls1, SeenRefs),
    findall(keyed(Ref, Positions),
            ( member(Ref-Positions, CanonicalKeys),
              \+ memberchk(Ref, SeenRefs) ),
            MissingKeys),
    insert_missing_key_declarations(Decls1, MissingKeys, Decls).

insert_missing_key_declarations(Decls, [], Decls) :- !.
insert_missing_key_declarations(Decls0, MissingKeys, Decls) :-
    (   append(Before, [semantic_type_rows(Rows) | After], Decls0)
    ->  append([Before, MissingKeys, [semantic_type_rows(Rows) | After]],
               Decls)
    ;   append(Decls0, MissingKeys, Decls)
    ).

rewrite_key_declarations([], _, Seen, [], Seen).
rewrite_key_declarations([Decl0 | Rest], CanonicalKeys, Seen0,
                         Decls, Seen) :-
    rewrite_one_key_declaration(Decl0, CanonicalKeys, Seen0,
                                Decl, Seen1),
    (   Decl == none
    ->  Decls = RestDecls
    ;   Decls = [Decl | RestDecls]
    ),
    rewrite_key_declarations(Rest, CanonicalKeys, Seen1,
                             RestDecls, Seen).

rewrite_one_key_declaration(col_type(Ref, Column, key(Type)),
                            CanonicalKeys, Seen, col_type(Ref, Column, Type),
                            Seen) :-
    memberchk(Ref-_, CanonicalKeys),
    !.
rewrite_one_key_declaration(keyed(Ref, _), CanonicalKeys, Seen0,
                            Decl, Seen) :-
    memberchk(Ref-Positions, CanonicalKeys),
    !,
    (   memberchk(Ref, Seen0)
    ->  Decl = none,
        Seen = Seen0
    ;   Decl = keyed(Ref, Positions),
        Seen = [Ref | Seen0]
    ).
rewrite_one_key_declaration(Decl, _, Seen, Decl, Seen).
