% Physical storage facts keyed by the canonical semantic type graph.
% rel/5 remains the runtime compatibility record; catalog serialization can
% rebuild canonical relation shapes from the rows below.

:- module(storage_projection,
          [ derive_storage_rows/3,
            replace_storage_type_rows/3,
            storage_rows_from_decls/2,
            project_storage_relplans/3,
            project_catalog_relplans/4
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('../0_rel_record',
              [ relplan_parts/6,
                relplan_storage_name/2,
                inferred_cols/3 ]).

%! derive_storage_rows(+Decls, +RelPlans, -Rows) is det.
%  Canonical owners receive physical relation, column, and key rows. A rel/5
%  plan without a canonical declaration remains a compatibility-only IDB.
derive_storage_rows(Decls, RelPlans, Rows) :-
    semantic_rows(Decls, SemanticRows),
    findall(Row,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Name/Arity, Kind, Columns, KeyOrNone,
                            StorageTypes),
              canonical_storage_owner(Decls, SemanticRows, Name, Arity,
                                      Owner),
              relplan_storage_name(RelPlan, TableName),
              storage_plan_row(SemanticRows, Owner, TableName, Kind, Columns,
                               StorageTypes, KeyOrNone, Row) ),
            Rows0),
    sort(Rows0, Rows),
    validate_storage_rows(SemanticRows, Rows).

semantic_rows(Decls, Rows) :-
    ( member(semantic_type_rows(Rows0), Decls) -> Rows = Rows0 ; Rows = [] ).

canonical_storage_owner(Decls, Rows, Name, Arity, Owner) :-
    memberchk(rel_module_decl(Name, Hash), Decls),
    Candidate = named(Hash, relation, Name),
    memberchk(declaration(Candidate, _, Name, relation, materialized), Rows),
    semantic_owner_arity(Rows, Candidate, Arity),
    !,
    Owner = Candidate.
canonical_storage_owner(Decls, Rows, Name, Arity, Owner) :-
    findall(Candidate,
            ( member(module_decl(_, Hash), Decls),
              Candidate = named(Hash, relation, Name),
              memberchk(declaration(Candidate, _, Name, relation,
                                    materialized), Rows),
              semantic_owner_arity(Rows, Candidate, Arity) ),
            ModuleOwners0),
    sort(ModuleOwners0, ModuleOwners),
    ModuleOwners = [Owner],
    !.
canonical_storage_owner(_, Rows, Name, Arity, Owner) :-
    findall(Id,
            ( member(declaration(Id, _, Name, relation, materialized), Rows),
              semantic_owner_arity(Rows, Id, Arity) ),
            Owners0),
    sort(Owners0, Owners),
    canonical_storage_owner_result(Name/Arity, Owners, Owner).

canonical_storage_owner_result(_, [], _) :- !, fail.
canonical_storage_owner_result(_, [Owner], Owner) :- !.
canonical_storage_owner_result(Ref, Owners, _) :-
    throw(unsupported_construct(canonical_storage_owner_conflict(Ref, Owners))).

semantic_owner_arity(Rows, Owner, Arity) :-
    findall(Position,
            member(member(_, Owner, Position, _, _), Rows),
            Positions),
    length(Positions, Arity).

storage_plan_row(_, Owner, TableName, Kind, _, _, _,
                 storage_relation(Owner, TableName, Kind)).
storage_plan_row(Rows, Owner, _, _, Columns, StorageTypes, _,
                 storage_column(MemberId, StorageType)) :-
    nth1(Position, Columns, Name),
    canonical_storage_member(Rows, Owner, Position, Name, MemberId,
                             LogicalTypeRef),
    nth1(Position, StorageTypes, RelPlanStorageType),
    canonical_storage_type(Rows, RelPlanStorageType, LogicalTypeRef,
                           StorageType).
storage_plan_row(Rows, Owner, _, _, Columns, _, key(KeyPositions),
                 storage_key(Owner, MemberId)) :-
    member(Position, KeyPositions),
    nth1(Position, Columns, Name),
    canonical_storage_member(Rows, Owner, Position, Name, MemberId, _).

canonical_storage_member(Rows, Owner, Position, Name, MemberId, TypeRef) :-
    ( member(member(MemberId, Owner, Position, Name, TypeRef), Rows)
    -> true
    ;  throw(unsupported_construct(
                   canonical_storage_member_missing(Owner, Position, Name)))
    ).

canonical_storage_type(_, int, _, primitive(int)) :- !.
canonical_storage_type(_, text, _, primitive(text)) :- !.
canonical_storage_type(_, float, _, primitive(float)) :- !.
canonical_storage_type(_, bool, _, primitive(bool)) :- !.
canonical_storage_type(_, json, _, primitive(json)) :- !.
canonical_storage_type(_, bytes, _, primitive(bytes)) :- !.
canonical_storage_type(Rows, ref(Name), TypeRef, reference(Target)) :- !,
    canonical_reference_target(Rows, Name, TypeRef, Target).
canonical_storage_type(Rows, idref(Name), TypeRef, relation_id(Target)) :- !,
    canonical_reference_target(Rows, Name, TypeRef, Target).
canonical_storage_type(_, list(_), TypeRef, list(Element)) :- !,
    semantic_list_element(TypeRef, Element).
canonical_storage_type(_, json_list(_), TypeRef, json_list(Element)) :- !,
    semantic_list_element(TypeRef, Element).
canonical_storage_type(_, StorageType, _, _) :-
    throw(unsupported_construct(canonical_storage_type_unknown(StorageType))).

canonical_reference_target(Rows, Name, TypeRef, Target) :-
    semantic_type_ref_target(TypeRef, LogicalTarget),
    reference_target_for_logical(Rows, Name, LogicalTarget, Target),
    !.
canonical_reference_target(Rows, Name, _, Target) :-
    findall(Candidate,
            member(declaration(Candidate, _, Name, relation, materialized),
                   Rows),
            Candidates0),
    sort(Candidates0, Candidates),
    canonical_storage_owner_result(Name, Candidates, Target).

reference_target_for_logical(Rows, Name, Target, Target) :-
    Target = named(_, relation, Name),
    memberchk(declaration(Target, _, Name, relation, materialized), Rows).
reference_target_for_logical(Rows, Name, LogicalTarget, Target) :-
    memberchk(derived_from(Target, LogicalTarget), Rows),
    Target = named(_, relation, Name),
    memberchk(declaration(Target, _, Name, relation, materialized), Rows).

semantic_type_ref_target(type_ref(TypeRef), Target) :- !,
    semantic_type_ref_target(TypeRef, Target).
semantic_type_ref_target(declaration(Target), Target) :- !.
semantic_type_ref_target(application(Target), Target) :- !.
semantic_type_ref_target(parameter(Target), Target) :- !.
semantic_type_ref_target(Target, Target).

semantic_list_element(TypeRef, Element) :-
    semantic_type_ref_target(TypeRef, Application),
    Application = application(_, [Element]),
    !.
semantic_list_element(TypeRef, _) :-
    throw(unsupported_construct(canonical_storage_list_target(TypeRef))).

validate_storage_rows(SemanticRows, Rows) :-
    validate_storage_relation_keys(Rows),
    validate_storage_column_keys(Rows),
    forall(member(storage_relation(Owner, _, _), Rows),
           require_storage_declaration(SemanticRows, Owner)),
    forall(member(storage_column(MemberId, StorageType), Rows),
           ( require_storage_member(SemanticRows, MemberId, _),
             validate_storage_type_targets(SemanticRows, StorageType) )),
    forall(member(storage_key(Owner, MemberId), Rows),
           validate_storage_key(SemanticRows, Rows, Owner, MemberId)).

validate_storage_relation_keys(Rows) :-
    findall(Owner-storage_relation(Owner, Table, Kind),
            member(storage_relation(Owner, Table, Kind), Rows),
            Pairs),
    validate_unique_storage_pairs(storage_relation, Pairs).

validate_storage_column_keys(Rows) :-
    findall(MemberId-storage_column(MemberId, StorageType),
            member(storage_column(MemberId, StorageType), Rows),
            Pairs),
    validate_unique_storage_pairs(storage_column, Pairs).

validate_unique_storage_pairs(Kind, Pairs0) :-
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Groups),
    maplist(validate_unique_storage_group(Kind), Groups).

validate_unique_storage_group(_, _-[_]) :- !.
validate_unique_storage_group(Kind, Key-Rows) :-
    sort(Rows, Distinct),
    ( Distinct = [_]
    -> true
    ;  throw(unsupported_construct(canonical_storage_conflict(Kind, Key,
                                                              Distinct)))
    ).

require_storage_declaration(Rows, Owner) :-
    ( memberchk(declaration(Owner, _, _, relation, materialized), Rows)
    -> true
    ;  throw(unsupported_construct(canonical_storage_owner_missing(Owner)))
    ).

require_storage_member(Rows, MemberId, Owner) :-
    ( memberchk(member(MemberId, Owner, _, _, _), Rows)
    -> true
    ;  throw(unsupported_construct(canonical_storage_member_id_missing(MemberId)))
    ).

validate_storage_key(SemanticRows, StorageRows, Owner, MemberId) :-
    require_storage_member(SemanticRows, MemberId, Owner),
    require_storage_relation(StorageRows, Owner),
    require_storage_column(StorageRows, MemberId).

require_storage_relation(Rows, Owner) :-
    ( memberchk(storage_relation(Owner, _, _), Rows)
    -> true
    ;  throw(unsupported_construct(canonical_storage_relation_missing(Owner)))
    ).

require_storage_column(Rows, MemberId) :-
    ( memberchk(storage_column(MemberId, _), Rows)
    -> true
    ;  throw(unsupported_construct(canonical_storage_column_missing(MemberId)))
    ).

validate_storage_type_targets(_, primitive(_)) :- !.
validate_storage_type_targets(Rows, reference(Target)) :- !,
    require_storage_type_target(Rows, Target).
validate_storage_type_targets(Rows, relation_id(Target)) :- !,
    require_storage_type_target(Rows, Target).
validate_storage_type_targets(Rows, list(Target)) :- !,
    require_storage_type_target(Rows, Target).
validate_storage_type_targets(Rows, json_list(Target)) :- !,
    require_storage_type_target(Rows, Target).

require_storage_type_target(_, primitive(_)) :- !.
require_storage_type_target(Rows, application(Constructor, Arguments)) :- !,
    memberchk(application(application(Constructor, Arguments), Constructor),
              Rows).
require_storage_type_target(Rows, Target) :-
    ( memberchk(declaration(Target, _, _, _, _), Rows)
    ; memberchk(parameter(Target, _, _, _), Rows)
    ),
    !.
require_storage_type_target(_, Target) :-
    throw(unsupported_construct(canonical_storage_target_missing(Target))).

replace_storage_type_rows(Decls0, Rows, Decls) :-
    exclude(is_storage_type_rows, Decls0, Decls1),
    append(Decls1, [storage_type_rows(Rows)], Decls).

is_storage_type_rows(storage_type_rows(_)).

storage_rows_from_decls(Decls, Rows) :-
    ( member(storage_type_rows(Rows0), Decls) -> Rows = Rows0 ; Rows = [] ).

%! project_storage_relplans(+Decls, +RelPlans0, -RelPlans) is det.
%  Preserve plan order. Canonical relations rebuild physical fields from the
%  semantic and storage rows; undeclared IDBs retain their compatibility plan.
project_storage_relplans(Decls, RelPlans0, RelPlans) :-
    semantic_rows(Decls, SemanticRows),
    storage_rows_from_decls(Decls, StorageRows),
    maplist(project_storage_relplan(Decls, SemanticRows, StorageRows, none),
            RelPlans0, RelPlans).

%! project_catalog_relplans(+Decls, +RelPlans0, +Modules, -RelPlans) is det.
%  Module markers select the module-qualified owner after catalog expansion.
project_catalog_relplans(Decls, RelPlans0, Modules, RelPlans) :-
    semantic_rows(Decls, SemanticRows),
    storage_rows_from_decls(Decls, StorageRows),
    maplist(project_storage_relplan_pair(Decls, SemanticRows, StorageRows),
            Modules, RelPlans0, RelPlans).

project_storage_relplan_pair(Decls, SemanticRows, StorageRows, Module,
                             RelPlan0, RelPlan) :-
    project_storage_relplan(Decls, SemanticRows, StorageRows, Module, RelPlan0,
                            RelPlan).

project_storage_relplan(Decls, SemanticRows, StorageRows, Module, RelPlan0,
                        RelPlan) :-
    relplan_parts(RelPlan0, Name/Arity, _, _, _, _),
    ( catalog_storage_owner(Decls, SemanticRows, Module, Name, Arity, Owner),
      memberchk(storage_relation(Owner, TableName, Kind), StorageRows)
    -> storage_owner_columns(SemanticRows, StorageRows, Owner, Columns,
                             StorageTypes),
       require_projected_arity(Owner, Arity, Columns),
       inferred_cols(Columns, StorageTypes, Cols),
       storage_owner_key(SemanticRows, StorageRows, Owner, KeyOrNone),
       RelPlan = rel(Name/Arity, TableName, Kind, Cols, KeyOrNone)
    ;  RelPlan = RelPlan0
    ).

require_projected_arity(Owner, Arity, Columns) :-
    length(Columns, ActualArity),
    ( ActualArity =:= Arity
    -> true
    ;  throw(unsupported_construct(
                   canonical_storage_arity_mismatch(Owner, Arity,
                                                    ActualArity)))
    ).

catalog_storage_owner(_, Rows, module(Hash), Name, Arity, Owner) :- !,
    Owner = named(Hash, relation, Name),
    memberchk(declaration(Owner, _, Name, relation, materialized), Rows),
    semantic_owner_arity(Rows, Owner, Arity).
catalog_storage_owner(Decls, Rows, _, Name, Arity, Owner) :-
    canonical_storage_owner(Decls, Rows, Name, Arity, Owner).

storage_owner_columns(SemanticRows, StorageRows, Owner, Columns,
                      StorageTypes) :-
    findall(Position-(Name-StorageType),
            ( member(member(MemberId, Owner, Position, Name, _), SemanticRows),
              memberchk(storage_column(MemberId, CanonicalStorage),
                        StorageRows),
              relplan_storage_type(CanonicalStorage, StorageType) ),
            Pairs0),
    keysort(Pairs0, Pairs),
    pairs_values(Pairs, NameStoragePairs),
    pairs_keys_values(NameStoragePairs, Columns, StorageTypes).

relplan_storage_type(primitive(Type), Type) :- !.
relplan_storage_type(reference(Target), ref(Name)) :- !,
    semantic_storage_type(Target, Name).
relplan_storage_type(relation_id(Target), idref(Name)) :- !,
    semantic_storage_type(Target, Name).
relplan_storage_type(list(Target), list(Type)) :- !,
    semantic_storage_type(Target, Type).
relplan_storage_type(json_list(Target), json_list(Type)) :- !,
    semantic_storage_type(Target, Type).

semantic_storage_type(primitive(Type), Type) :- !.
semantic_storage_type(named(_, _, Name), Name) :- !.
semantic_storage_type(application(named(_, _, Constructor), Arguments), Type) :-
    !,
    maplist(semantic_storage_type, Arguments, SurfaceArguments),
    Type =.. [Constructor | SurfaceArguments].
semantic_storage_type(Type, Type).

storage_owner_key(SemanticRows, StorageRows, Owner, KeyOrNone) :-
    findall(Position,
            ( member(storage_key(Owner, MemberId), StorageRows),
              memberchk(member(MemberId, Owner, Position, _, _), SemanticRows) ),
            Positions0),
    sort(Positions0, Positions),
    ( Positions == [] -> KeyOrNone = none ; KeyOrNone = key(Positions) ).
