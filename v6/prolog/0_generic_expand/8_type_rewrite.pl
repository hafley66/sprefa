% The collapsed list column keeps a record of the type it collapsed FROM, the
% option_column/3 precedent: nothing downstream can read `int` and know better.
replace_generic_types(Decls0, Instances, Decls) :-
    derived_application_rewrites(Decls0, DerivedApplications),
    append(Instances, DerivedApplications, Rewrites),
    maplist(replace_generic_decl(Rewrites), Decls0, Rewritten),
    findall(list_column(Ref, Column, Type),
            ( member(col_type(Ref, Column, Type), Decls0),
              list_flavor(Type),
              memberchk(Type, Instances) ),
            ListColumns),
    append(Rewritten, ListColumns, Decls).

derived_application_rewrites(Decls, Rewrites) :-
    findall(derived_application(Type, Name),
            ( member(compiler_derived_type_application(Type), Decls),
              canonical_type_name(Type, Name) ),
            Rewrites0),
    sort(Rewrites0, Rewrites).

replace_generic_decl(Instances,
                     col_type(Ref, Column, Type0),
                     col_type(Ref, Column, Type)) :-
    !,
    replace_generic_type(Type0, Instances, Type).
replace_generic_decl(_, Decl, Decl).

retarget_type_decl_mirrors(Decls0, Decls) :-
    maplist(retarget_type_decl_mirror(Decls0), Decls0, Decls).

% The mirror states the rel's stored columns, so it is re-read whole from the
% expanded col_type rows: a rename and a drop land by the same read.
retarget_type_decl_mirror(Decls,
                          type_decl(RelName, Specs0),
                          type_decl(RelName, Specs)) :-
    !,
    (   expanded_relation_specs(Decls, RelName, Rebuilt)
    ->  Specs = Rebuilt
    ;   Specs = Specs0
    ).
retarget_type_decl_mirror(_, Decl, Decl).

expanded_relation_specs(Decls, RelName, Specs) :-
    once(member(col_type(RelName/Arity, _, _), Decls)),
    findall(col(Column, Type),
            ( member(col_type(RelName/Arity, Column, Stored), Decls),
              mirror_column_type(Decls, Stored, Type) ),
            Specs),
    length(Specs, Arity).
% A rel reached in column position needs stored columns: identity is key(...)
% or the full row, and a zero-column row has neither.
expanded_relation_specs(Decls, RelName, _) :-
    \+ member(col_type(RelName/_, _, _), Decls),
    memberchk(option_column(RelName/_, _, _), Decls),
    throw(unsupported_construct(
            reference_target_has_no_columns(RelName/0))).

mirror_column_type(Decls, Type, int) :-
    memberchk(enum_decl(Type, _), Decls),
    !.
mirror_column_type(_, Type, Type).

replace_generic_type(option(Type0), Instances, option(Name)) :-
    !,
    replace_option_element(Type0, Instances, Name).
% A bare list column stores its entity id and is SPELLED list(Element) to the
% relplan; the named flavors have no boundary render and still collapse.
replace_generic_type(list(Element0), Instances, list(Element)) :-
    memberchk(list(Element0), Instances),
    !,
    replace_generic_type(Element0, Instances, Element).
% Keep annotation syntax attached to the substituted concrete type. Compiler
% relation execution and evidence consumption happen after this phase.
replace_generic_type(annotated_type(Type0, Applications0), Instances,
                     annotated_type(Type, Applications)) :-
    !,
    replace_generic_type(Type0, Instances, Type),
    Applications = Applications0.

replace_generic_type(Type, Instances, Name) :-
    memberchk(derived_application(Type, Name), Instances),
    !.

replace_generic_type(Type, Instances, int) :-
    list_flavor(Type),
    memberchk(Type, Instances),
    !.
replace_generic_type(Type, _, Type).

% option(list(T)) keeps the entity name so its companion split rel targets the
% minted list; other generic option elements hand back to the general rewrite.
replace_option_element(Type0, Instances, Name) :-
    (   list_flavor(Type0),
        memberchk(Type0, Instances)
    ->  canonical_type_name(Type0, Name)
    ;   replace_generic_type(Type0, Instances, Name)
    ).

% Readable stem plus a 64-bit SHA-256 prefix.  The digest input is the complete
% length-prefixed structural encoding.  `validate_generated_name_collisions/3`
% rejects a truncated-digest collision before lowering any declaration.
canonical_type_name(Type, Name) :-
    canonical_type_encoding(Type, Encoding),
    readable_stem(Type, Stem),
    crypto_data_hash(Encoding, FullDigest, [algorithm(sha256)]),
    sub_atom(FullDigest, 0, 16, _, Digest),
    atomic_list_concat(['__gen_', Stem, Digest], '_', Name).

canonical_type_encoding(Type, Encoding) :-
    type_encoding_codes(Type, Codes),
    atom_codes(Encoding, Codes).

type_encoding_codes(Type, Codes) :-
    atom(Type), !,
    atom_codes(Type, AtomCodes), length(AtomCodes, Length),
    number_codes(Length, LengthCodes),
    append([[0'a], LengthCodes, [0':], AtomCodes], Codes).
type_encoding_codes([], [0'l, 0':]) :- !.
type_encoding_codes(Type, Codes) :-
    compound(Type),
    Type =.. [Constructor | Args],
    atom_codes(Constructor, ConstructorCodes), length(ConstructorCodes, Length),
    number_codes(Length, LengthCodes),
    maplist(type_encoding_codes, Args, ArgCodes), append(ArgCodes, FlatArgs),
    length(Args, Arity), number_codes(Arity, ArityCodes),
    append([[0'c], LengthCodes, [0':], ConstructorCodes, [0'/], ArityCodes,
            [91], FlatArgs, [93]], Codes).

readable_stem(Type, Stem) :-
    atom(Type), !, Stem = Type.
readable_stem([], Stem) :- !, Stem = 'empty'.
readable_stem(Type, Stem) :-
    is_list(Type), !,
    maplist(readable_stem, Type, Stems),
    atomic_list_concat(Stems, '_', Stem).
readable_stem(Type, Stem) :-
    Type =.. [Constructor | Args],
    maplist(readable_stem, Args, Stems),
    atomic_list_concat([Constructor | Stems], '_', Stem).

generated_generic_name(Name) :-
    atom(Name), sub_atom(Name, 0, _, _, '__gen_').
