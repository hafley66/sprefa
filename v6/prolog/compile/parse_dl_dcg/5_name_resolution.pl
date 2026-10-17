resolve_module_path_collisions(Decls0, Decls) :-
    findall(Ref-Segs, member(rel_path_decl(Ref, Segs), Decls0), Paths),
    ( Paths == []
    -> Decls = Decls0
    ; reserved_rel_names(Decls0, Paths, Reserved),
      foldl(disambiguate_module_path(Reserved), Paths, Decls0, Decls)
    ).

disambiguate_module_path(Reserved, Name/Arity-Segs, Decls0, Decls) :-
    ( memberchk(Name, Reserved)
    -> variant_sha1(Segs, Sha),
       sub_atom(Sha, 0, 16, _, Digest),
       atomic_list_concat([Name, '__', Digest], Digested),
       ( lookup_column_order(Name, Cols)
       -> record_cols(Digested, Cols)
       ; true
       ),
       maplist(rename_decl_ref(Name/Arity, Digested/Arity), Decls0, Decls)
    ; Decls = Decls0
    ).

rename_decl_ref(Old, New, Decl0, Decl) :-
    Decl0 =.. [F, Old | Args],
    memberchk(F, [col_type, kind, keep, keyed, rel_path_decl]),
    !,
    Decl =.. [F, New | Args].
rename_decl_ref(_, _, Decl, Decl).

reserved_rel_names(Decls, Paths, Reserved) :-
    findall(Name,
            ( declared_rel_name(Decls, Ref, Name),
              \+ memberchk(Ref-_, Paths)
            ; minted_rel_name(Decls, Name)
            ),
            Names),
    sort(Names, Reserved).

declared_rel_name(Decls, Name/Arity, Name) :-
    member(Decl, Decls),
    Decl =.. [F, Name/Arity | _],
    memberchk(F, [col_type, kind, keyed, keep]).
declared_rel_name(Decls, enum_decl(Name), Name) :-
    member(enum_decl(Name, _), Decls).

minted_rel_name(Decls, Minted) :-
    ( member(col_type(Parent/_, Col, option(_)), Decls),
      atomic_list_concat([Parent, '__', Col], Minted)
    ; member(col_type(_, _, option(Element)), Decls),
      atom(Element),
      atomic_list_concat(['__opt_', Element], Minted)
    ; member(enum_decl(Rel, Variants), Decls),
      ( tree_leaf(';', Variants, Variant),
        Variant =.. [Name | _],
        atomic_list_concat([Rel, Name], '_', Minted)
      ; tag_rel_name(Rel, Minted)
      )
    ).


normalize_relation_value_decls(Decls0, Decls) :-
    findall(Name,
            ( declared_column_type_name(Decls0, Name),
              relation_schema(Decls0, Name, _, _) ),
            Names0),
    sort(Names0, ValueNames),
    normalize_relation_value_decls(Decls0, ValueNames, [], Decls).

normalize_relation_value_decls([], _, _, []).
normalize_relation_value_decls([Head | Rest], VNames, Seen, Out) :-
    ( Head = col_type(Name/Arity, _, _),
      memberchk(Name, VNames),
      \+ memberchk(Name, Seen)
    -> relation_schema([Head | Rest], Name, Name/Arity, Specs),
       Out = [type_decl(Name, Specs), Head | More],
       Seen1 = [Name | Seen]
    ; Out = [Head | More], Seen1 = Seen
    ),
    normalize_relation_value_decls(Rest, VNames, Seen1, More).

relation_schema(Decls, Name, Ref, Specs) :-
    once(member(col_type(Name/Arity, _, _), Decls)),
    Ref = Name/Arity,
    findall(col(Col, Type), member(col_type(Ref, Col, Type), Decls), Specs),
    length(Specs, Arity).

declared_column_type_name(Decls, Name) :-
    ( member(col_type(_, _, Type), Decls),
      ( Name = Type
      ; column_element_type_name(Type, Name)
      ; key_option_relation_type_name(Type, Name)
      )
    ; member(sh_decl(_, Ins, Outs, _), Decls),
      append(Ins, Outs, Cols),
      member(col(_, Name), Cols)
    ; member(enum_decl(_, Variants), Decls),
      tree_leaf(';', Variants, Variant),
      Variant =.. [_ | Fields],
      member(_:Name, Fields)
    ),
    \+ scalar_column_type(Name).

scalar_column_type(T) :- member(T, [int, text, json, bool, float, bytes]).

% key(option(Relation)) needs the relation mirror before option expansion.
% Descend only through key and nested option wrappers.
key_option_relation_type_name(key(Inner), Name) :- !,
    option_relation_type_name(Inner, Name).

option_relation_type_name(option(Inner), Name) :- !,
    option_relation_type_name(Inner, Name).
option_relation_type_name(Name, Name) :- atom(Name).


% RULING sh_bind_surface_removed: the whole statement is consumed with quotes
% and backticks respected, so a template's own `.` cannot end it early.
