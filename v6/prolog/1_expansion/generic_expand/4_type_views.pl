% Source terms are accepted here, but normalized rows are the only output; ids
% derive from labels and ordinals, not source order.
generic_type_ir(Decls, Rows) :-
    normalized_type_rows(Decls, Rows).

%! schema_member_rows(+Decls, -Rows) is det.
%
% This is a parallel compiler metadata view.  It deliberately stays out of
% semantic_type_rows/1 so the existing runtime declaration term and all
% ordinary type artifacts remain byte-stable while catalog/typegen consumers
% can request the richer role-bearing schema.
schema_member_rows(Decls, Rows) :-
    canonical_member_rows(Decls, MemberRows),
    findall(Row,
            ( member(member(MemberId, OwnerId, Position, Name, TypeRef),
                     MemberRows),
              canonical_member_type_term(Decls, TypeRef, AuthoredType),
              schema_value_type_id(Decls, TypeRef, ValueTypeId),
              schema_member_roles(Decls, OwnerId, Position, Name, TypeRef,
                                  Roles),
              Row = schema_member(MemberId, OwnerId, Position, Name,
                                  AuthoredType, ValueTypeId, Roles) ),
            Unsorted),
    sort(Unsorted, Rows).

canonical_member_rows(Decls, Rows) :-
    member(semantic_type_rows(SemanticRows), Decls),
    !,
    findall(member(MemberId, OwnerId, Position, Name, TypeRef),
            member(member(MemberId, OwnerId, Position, Name, TypeRef),
                   SemanticRows),
            Rows).
canonical_member_rows(Decls, Rows) :-
    normalized_member_rows(Decls, Rows).

canonical_member_type_term(_, type_ref(parameter(ParameterId)), ParameterId) :- !.
canonical_member_type_term(_, type_ref(primitive(Type)), Type) :- !.
canonical_member_type_term(_, type_ref(named(Type)), Type) :- !.
canonical_member_type_term(Decls, type_ref(TypeId), Type) :-
    semantic_type_term(Decls, TypeId, Type).

%! type_relation_rows(+Decls, -Rows) is det.
%  Rows is the concatenation of normalized schema_member/7 and
%  type_relation/5 terms.  A relation without a Self member remains an
%  ordinary relation row and does not enter trait validation.
type_relation_rows(Decls, Rows) :-
    schema_member_rows(Decls, MemberRows),
    findall(Row,
            type_relation_row(Decls, MemberRows, Row),
            RelationRows),
    compiler_metadata_rows(Decls, MetadataRows),
    append([MemberRows, RelationRows, MetadataRows], Rows0),
    sort(Rows0, Rows).

compiler_metadata_rows(Decls, Rows) :-
    member(compiler_type_metadata(MetadataRows, ClosureRows, _), Decls),
    compiler_evidence_rows(Decls, MetadataRows, ClosureRows, EvidenceRows),
    Rows = EvidenceRows,
    !.
compiler_metadata_rows(Decls, Rows) :-
    member(compiler_type_metadata(MetadataRows, ClosureRows), Decls),
    compiler_evidence_rows(Decls, MetadataRows, ClosureRows, EvidenceRows),
    Rows = EvidenceRows,
    !.
compiler_metadata_rows(_, []).

% Evidence is carried beside the target-independent relation metadata so the
% type artifact doors can make the same impl-emission decision as the
% compiler.  One row names the relation owner and carries one complete,
% ground compiler-plane fact.  A fact is complete when its functor is the
% owner's relation name; arity and keyed-position validation remain the
% emitter's responsibility because the relation row owns that contract.
compiler_evidence_rows(Decls, MetadataRows, ClosureRows, EvidenceRows) :-
    findall(type_relation_evidence(OwnerId, Evidence),
            ( member(type_relation(OwnerId, _Self, _Inputs, _Return, _Keys),
                     MetadataRows),
              relation_owner_name(OwnerId, OwnerName),
              compiler_evidence_owner_unambiguous(Decls, OwnerName),
              compiler_evidence_arity(MetadataRows, OwnerId, Arity),
              member(Evidence, ClosureRows),
              compound(Evidence),
              functor(Evidence, OwnerName, Arity),
              ground(Evidence) ),
            Unsorted),
    sort(Unsorted, EvidenceRows).

% Compiler closure terms carry a surface functor, while catalog identity is
% module-qualified.  A name declared by more than one loaded module has no
% source-module field left on that closure term, so it cannot authorize an
% owner-specific Rust impl.  Suppress that evidence until the compiler-plane
% evaluator transports the owner identity itself.
compiler_evidence_owner_unambiguous(Decls, OwnerName) :-
    findall(ModuleHash, member(rel_module_decl(OwnerName, ModuleHash), Decls),
            Hashes0),
    sort(Hashes0, Hashes),
    ( Hashes = [] ; Hashes = [_] ).

compiler_evidence_arity(MetadataRows, OwnerId, Arity) :-
    findall(MemberId,
            member(schema_member(MemberId, OwnerId, _, _, _, _, _),
                   MetadataRows),
            MemberIds),
    length(MemberIds, Arity).

relation_owner_name(named(_, relation, Name), Name) :- !.
relation_owner_name(relation(Name), Name).

metadata_relation_owner_identity(named(ModuleId, relation, Name), ModuleId,
                                 Name) :- !.
metadata_relation_owner_identity(relation(Name), local, Name).

%! schema_member_transport_rows(+CatalogRows, +RelationRows, -Rows) is det.
%  Add typed child rows and the catalog-column bridge used at artifact
%  boundaries.  The normalized schema_member/7 and type_relation/5 terms stay
%  internal; these rows carry no Prolog list-valued columns.
schema_member_transport_rows(CatalogRows, RelationRows, Rows) :-
    findall(schema_member_column(ColumnId, MemberId),
            schema_member_catalog_column(CatalogRows, RelationRows,
                                         MemberId, ColumnId),
            ColumnRows0),
    sort(ColumnRows0, ColumnRows),
    findall(schema_member_role(MemberId, Ordinal, Role, Argument),
            schema_member_role_row(RelationRows, MemberId, Ordinal, Role,
                                   Argument),
            RoleRows),
    findall(type_relation_input(OwnerId, Ordinal, MemberId),
            ( member(type_relation(OwnerId, _, InputMemberIds, _, _),
                     RelationRows),
              nth1(Ordinal, InputMemberIds, MemberId) ),
            InputRows),
    findall(type_relation_key(OwnerId, Ordinal, MemberId),
            ( member(type_relation(OwnerId, _, _, _, KeyMemberIds),
                     RelationRows),
              nth1(Ordinal, KeyMemberIds, MemberId) ),
            KeyRows),
    findall(type_relation_owner(OwnerId, ModuleId, Name),
            ( member(type_relation(OwnerId, _, _, _, _), RelationRows),
              metadata_relation_owner_identity(OwnerId, ModuleId, Name) ),
            OwnerRows0),
    sort(OwnerRows0, OwnerRows),
    append([ColumnRows, RoleRows, InputRows, KeyRows, OwnerRows],
           Rows0),
    sort(Rows0, Rows).

schema_member_catalog_column(CatalogRows, RelationRows, MemberId, ColumnId) :-
    member(schema_member(MemberId, OwnerId, Position, _, _, _, _),
           RelationRows),
    id_kind_name(OwnerId, relation, OwnerName),
    member(row(RelId, _, _, OwnerName, rel, _, _, _, _, _, _), CatalogRows),
    member(row(ColumnId, RelId, Ordinal, _, column, _, _, _, _, _, _),
           CatalogRows),
    Position is Ordinal + 1.

schema_member_role_row(RelationRows, MemberId, Ordinal, Role, Argument) :-
    member(schema_member(MemberId, _, _, _, _, _, Roles), RelationRows),
    nth1(Ordinal, Roles, RoleTerm),
    transport_role(RoleTerm, Role, Argument).

transport_role(self_subject, self_subject, '') :- !.
transport_role(key, key, '') :- !.
transport_role(return, return, '') :- !.
transport_role(anonymous_owner(Path), anonymous_owner, Path).

normalized_member_rows(Decls, Rows) :-
    findall(Row, normalized_member_row(Decls, Row), Rows0),
    first_member_row_per_id(Rows0, Rows).

authored_member_type(Decls, OwnerId, Position, Type) :-
    id_kind_name(OwnerId, relation, OwnerName),
    owner_member_carrier(Decls, OwnerName, carrier(Kind, Specs, _)),
    carrier_member_type(Kind, Position, Specs, Type).

carrier_member_type(template, Position, Specs, Type) :-
    nth1(Position, Specs, column(_, Type)).
carrier_member_type(type_decl, Position, Specs, Type) :-
    nth1(Position, Specs, col(_, Type)).
carrier_member_type(pairs, Position, Pairs, Type) :-
    nth1(Position, Pairs, _-Type).

% Keyed by owner, not by member. Same cache scope as semantic_decl_id/4, which
% is what makes one Decls list per scope safe to assume.
owner_member_carrier(_Decls, OwnerName, Carrier) :-
    atom(OwnerName),
    nb_current(generic_semantic_id_cache, cache(_, _, OwnerIndex)),
    OwnerIndex \== none,
    !,
    (   get_assoc(OwnerName, OwnerIndex, Carrier)
    ->  true
    ;   Carrier = carrier(pairs, [], [])
    ).
owner_member_carrier(Decls, OwnerName, Carrier) :-
    owner_member_carrier_uncached(Decls, OwnerName, Carrier).

%! owner_carrier_index(+Decls, -Index) is det.
% Four Decls passes for the whole program, where the per-owner form below is
% four passes per owner. `none` when a declaration names its owner with an
% unbound term, which get_assoc/3 cannot answer the way member/2 does.
owner_carrier_index(Decls, Index) :-
    findall(Name-template(Specs),
            ( member(rel_template(Segments, _, Specs), Decls),
              atomic_list_concat(Segments, '__', Name) ),
            TemplateRows),
    findall(Name-type_decl(Specs),
            member(type_decl(Name, Specs), Decls),
            TypeDeclRows),
    findall(Name-column(ColumnName, ColumnType),
            member(col_type(Name/_, ColumnName, ColumnType), Decls),
            ColumnRows),
    findall(Name-key_positions(Positions),
            member(keyed(Name/_, Positions), Decls),
            KeyedRows),
    append([TemplateRows, TypeDeclRows, ColumnRows, KeyedRows], Rows),
    (   forall(member(Name0-_, Rows), atom(Name0))
    ->  keysort(Rows, Sorted),
        group_pairs_by_key(Sorted, Grouped),
        maplist(owner_carrier_entry, Grouped, Entries),
        list_to_assoc(Entries, Index)
    ;   Index = none
    ).

% keysort/2 is stable, so each owner's items keep the order the scans they
% replace read them in, and the first template or type_decl still wins.
owner_carrier_entry(Name-Items, Name-carrier(Kind, Specs, Keyed)) :-
    (   memberchk(template(TemplateSpecs), Items)
    ->  Kind = template, Specs = TemplateSpecs
    ;   memberchk(type_decl(DeclSpecs), Items)
    ->  Kind = type_decl, Specs = DeclSpecs
    ;   Kind = pairs,
        findall(ColumnName-ColumnType,
                member(column(ColumnName, ColumnType), Items), Specs)
    ),
    findall(Positions, member(key_positions(Positions), Items), Keyed).

owner_member_carrier_uncached(Decls, OwnerName, carrier(Kind, Specs, Keyed)) :-
    (   member(rel_template(Segments, _, TemplateSpecs), Decls),
        atomic_list_concat(Segments, '__', OwnerName)
    ->  Kind = template, Specs = TemplateSpecs
    ;   member(type_decl(OwnerName, DeclSpecs), Decls)
    ->  Kind = type_decl, Specs = DeclSpecs
    ;   Kind = pairs,
        findall(ColumnName-ColumnType,
                member(col_type(OwnerName/_, ColumnName, ColumnType), Decls),
                Specs)
    ),
    findall(Positions, member(keyed(OwnerName/_, Positions), Decls), Keyed).

schema_value_type_id(_Decls, type_ref(parameter(ParameterId)), ParameterId) :-
    !.
schema_value_type_id(_Decls, type_ref(declaration(TypeId)), TypeId) :- !.
schema_value_type_id(_Decls, type_ref(application(TypeId)), TypeId) :- !.
schema_value_type_id(Decls, type_ref(primitive(Type)), TypeId) :-
    !,
    ( Type == type
    -> primitive_id(type, TypeId)
    ;  semantic_type_id(Decls, Type, TypeId)
    ).
schema_value_type_id(Decls, type_ref(named(Type)), TypeId) :-
    !,
    ( Type == type
    -> primitive_id(type, TypeId)
    ;  semantic_type_id(Decls, Type, TypeId)
    ).
schema_value_type_id(Decls, Type, TypeId) :-
    semantic_type_id(Decls, Type, TypeId).

normalized_member_role_row(Decls, member_role(MemberId, Role)) :-
    normalized_member_row(Decls,
                          member(MemberId, OwnerId, Position, Name, TypeRef)),
    schema_member_roles_from_carriers(Decls, OwnerId, Position, Name, TypeRef,
                                      Roles),
    member(Role, Roles).

schema_member_roles(Decls, OwnerId, Position, Name, TypeRef, Roles) :-
    MemberId = member(OwnerId, Position, Name),
    (   member(semantic_type_rows(SemanticRows), Decls),
        findall(Role,
                member(member_role(MemberId, Role), SemanticRows),
                CanonicalRoles),
        CanonicalRoles \== []
    ->  sort(CanonicalRoles, Roles)
    ;   schema_member_roles_from_carriers(Decls, OwnerId, Position, Name,
                                          TypeRef, Roles)
    ).

schema_member_roles_from_carriers(Decls, OwnerId, Position, Name, TypeRef,
                                  Roles) :-
    ( Name == 'Self' -> SelfRoles = [self_subject] ; SelfRoles = [] ),
    ( owner_key_position(Decls, OwnerId, Position)
    -> KeyRoles = [key]
    ;  KeyRoles = []
    ),
    ( Name == return -> ReturnRoles = [return] ; ReturnRoles = [] ),
    ( ( anonymous_owner_path_for_member(Decls, OwnerId, Position, Path)
      ; anonymous_owner_path(TypeRef, Path)
      )
    -> AnonymousRoles = [anonymous_owner(Path)]
    ;  AnonymousRoles = []
    ),
    findall(DerivedRole,
            ( id_kind_name(OwnerId, relation, OwnerName),
              member(compiler_derived_member_role(OwnerName, Position,
                                                  Role, Argument), Decls),
              compiler_derived_role_term(Role, Argument, DerivedRole) ),
            DerivedRoles),
    append([SelfRoles, KeyRoles, ReturnRoles, AnonymousRoles, DerivedRoles],
           RawRoles),
    list_to_set(RawRoles, Roles).

compiler_derived_role_term(Role, '', Role) :- !.
compiler_derived_role_term(Role, Argument, RoleTerm) :-
    RoleTerm =.. [Role, Argument].

anonymous_owner_path_for_member(Decls, OwnerId, Position, Path) :-
    authored_member_type(Decls, OwnerId, Position, Type),
    anonymous_owner_source_path(Type, Path).

anonymous_owner_source_path(anonymous_product(Path, _), Path) :- !.
anonymous_owner_source_path(anonymous_sum(Path, _), Path) :- !.
anonymous_owner_source_path(anonymous(Path, _), Path) :- !.

owner_key_position(Decls, OwnerId, Position) :-
    id_kind_name(OwnerId, relation, OwnerName),
    owner_member_carrier(Decls, OwnerName, carrier(_, _, KeyedPositions)),
    member(Positions, KeyedPositions),
    memberchk(Position, Positions).

anonymous_owner_path(anonymous_product(Path, _), Path) :- !.
anonymous_owner_path(anonymous_sum(Path, _), Path) :- !.
anonymous_owner_path(anonymous(Path, _), Path) :- !.
anonymous_owner_path(type_ref(named(Type)), Path) :-
    !,
    anonymous_owner_path(Type, Path).

type_relation_row(_Decls, MemberRows,
                  type_relation(OwnerId, SelfMemberId, InputMemberIds,
                                ReturnMemberOrNone, KeyMemberIds)) :-
    setof(Owner,
          MemberId^Position^Name^AuthoredType^ValueType^Roles^
          member(schema_member(MemberId, Owner, Position, Name,
                                AuthoredType, ValueType, Roles), MemberRows),
          Owners),
    member(OwnerId, Owners),
    findall(Position-MemberId-Name-Type,
            member(schema_member(MemberId, OwnerId, Position, Name, Type,
                                  _, _), MemberRows), MemberPairs0),
    sort(MemberPairs0, MemberPairs),
    self_members(MemberPairs, SelfMembers),
    self_member_or_none(SelfMembers, SelfMemberId),
    input_member_ids(MemberPairs, InputMemberIds),
    return_member_or_none(MemberPairs, ReturnMemberOrNone),
    key_member_ids(MemberRows, OwnerId, KeyMemberIds).

self_members(MemberPairs, SelfMembers) :-
    findall(Position-MemberId-Type,
            member(Position-MemberId-'Self'-Type, MemberPairs), SelfMembers).

self_member_or_none([], none).
self_member_or_none([_-MemberId-_|_], MemberId).

input_member_ids(MemberPairs, InputMemberIds) :-
    findall(MemberId,
            ( member(_-MemberId-Name-_, MemberPairs),
              Name \== 'Self', Name \== return ),
            InputMemberIds).

return_member_or_none(MemberPairs, MemberId) :-
    ( member(_-MemberId-return-_, MemberPairs) -> true ; MemberId = none ).

key_member_ids(MemberRows, OwnerId, KeyMemberIds) :-
    findall(MemberId,
            ( member(schema_member(MemberId, OwnerId, _, _, _, _, Roles),
                     MemberRows),
              memberchk(key, Roles) ),
            KeyMemberIds).

% One compile asks for the same rows five times: expand_user_templates/4 and
% freeze_type_rows/2 each rebuild them, the generic pipeline runs twice, and
% expand_program_run/4 freezes once more. Keyed by variant, so an entry is
% only ever reused for a Decls list that would rebuild the same rows.
:- thread_local type_row_memo/3.

reset_type_row_memo :-
    retractall(type_row_memo(_, _, _)).

normalized_type_rows(Decls, Rows) :-
    variant_sha1(Decls, Hash),
    (   type_row_memo(Hash, MemoDecls, MemoRows),
        MemoDecls =@= Decls
    ->  Rows = MemoRows
    ;   normalized_type_rows_rebuilt(Decls, Rows),
        % A non-ground row set would lose its variable identity to assertz/1.
        (   ground(Rows)
        ->  assertz(type_row_memo(Hash, Decls, Rows))
        ;   true
        )
    ).

normalized_type_rows_rebuilt(Decls, Rows) :-
    owner_carrier_index(Decls, OwnerIndex),
    setup_call_cleanup(
        nb_setval(generic_semantic_id_cache, cache(t, t, OwnerIndex)),
        normalized_type_rows_cached(Decls, Rows),
        nb_delete(generic_semantic_id_cache)).

normalized_type_rows_cached(Decls, Rows) :-
    findall(Row, normalized_declaration_row(Decls, Row), DeclarationRows),
    findall(Row, normalized_parameter_row(Decls, Row), ParameterRows),
    findall(Row, normalized_member_row(Decls, Row), MemberRows0),
    first_member_row_per_id(MemberRows0, MemberRows),
    findall(Row, normalized_member_role_row(Decls, Row), MemberRoleRows),
    findall(Row, normalized_constraint_row(Decls, Row), ConstraintRows),
    normalized_application_rows(Decls, ApplicationRows),
    normalized_derivation_rows(Decls, ApplicationRows, DerivationRows),
    append([DeclarationRows, ParameterRows, MemberRows, MemberRoleRows,
            ConstraintRows, ApplicationRows, DerivationRows], Unsorted),
    sort(Unsorted, Rows).
