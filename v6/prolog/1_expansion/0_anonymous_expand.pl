% 0_anonymous_expand.pl: anonymous product/sum type identity and materialization.
%
% Anonymous products and sums are source-level elisions: `(a: int, b: text)`
% parses to `product_type([field(a, int), field(b, text)])` and
% `(Ok(v: T); Err(m: text))` to `sum_type([variant(Ok, [field(v, T)]), ...])`.
% They are legal anywhere `type_expr//1` is accepted.  This phase runs AFTER
% concrete generic substitution (so field types are already concrete) and BEFORE
% option/enum lowering, and mints an owner-scoped semantic identity
%
%   anonymous(OwnerSemanticTypeId, SitePath, SpecializedShape)
%
% plus an ordinary generated `type_decl` (product) or `enum_decl` (sum) whose
% internal name is diagnostic-only.  `SitePath` is the recursive member-name
% path plus wrapper/application argument ordinals from the owner's top-level
% type to this anonymous type; it does not depend on any unrelated declaration.
%
% Materialized sums become `enum_decl` before enum context is computed, so the
% existing enum expansion lowers them.  Full runtime value construction and
% storage belong to @anonymous-product-values / @anonymous-sum-values.

:- module(anonymous_expand,
          [ expand_anonymous_decls/2,
            anonymous_owner_path/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(crypto)).
:- use_module('0_type_ids',
              [ decl_id/4, id_kind_name/3, semantic_type_id_text/2 ]).

:- op(1150, xfx, <-).

% ═══ entry ═══════════════════════════════════════════════════════════════════

expand_anonymous_decls(Decls0, Decls) :-
    mint_all(Decls0, RewrittenDecls, AnonymousRows),
    merge_anonymous_type_rows(AnonymousRows, RewrittenDecls, RowedDecls),
    rewrite_anonymous_semantic_rows(RowedDecls, Decls).

% ═══ walk over declared types ════════════════════════════════════════════════
%
% Two carriers describe column types: col_type/3 (ordinary columns) and
% type_decl/2 (a relation-valued type's declared members).  Both are walked so
% an anonymous type in either position mints one identity and one materialized
% declaration.  The generated name is a digest of the identity, so re-minting
% the same site is idempotent and duplicate declarations collapse on sort.

mint_all(Decls0, Decls, Rows) :-
    mint_col_types(Decls0, Decls1, ColumnRows),
    mint_type_decl_specs(Decls1, Decls2, SpecRows),
    % Materialized declarations follow author declarations, matching the
    % generated-name ordering convention used elsewhere in expansion.
    partition(minted_anonymous_decl(Decls2), Decls2, Minted, Author),
    sort(Minted, UniqueMinted),
    append(Author, UniqueMinted, Decls),
    append(ColumnRows, SpecRows, UnsortedRows),
    sort(UnsortedRows, Rows).

minted_anonymous_decl(Decls, type_decl(Name, _)) :-
    memberchk(anonymous_generated_decl(Name), Decls), !.
minted_anonymous_decl(Decls, enum_decl(Name, _)) :-
    memberchk(anonymous_generated_decl(Name), Decls), !.
minted_anonymous_decl(Decls, semantic_decl_module(_, Name, _)) :-
    memberchk(anonymous_generated_decl(Name), Decls), !.
minted_anonymous_decl(_, anonymous_generated_decl(_)).

mint_col_types(Decls0, Decls, Rows) :-
    maplist(mint_col_type(Decls0), Decls0, DeclLists, RowLists),
    append(DeclLists, FlatDecls),
    append(RowLists, FlatRows),
    Decls = FlatDecls,
    Rows = FlatRows.

mint_col_type(AllDecls, Decl, NewDecls, Rows) :-
    Decl = col_type(Ref, Column, Type0),
    type_contains_anonymous(Type0),
    !,
    ref_name(Ref, OwnerName),
    semantic_owner_id(AllDecls, OwnerName, Owner),
    anonymous_mint(AllDecls, Owner, [Column], Type0, Type, ExtraDecls, Rows),
    NewDecls = [col_type(Ref, Column, Type) | ExtraDecls].
mint_col_type(_, Decl, [Decl], []).

ref_name(Name/_, Name) :- !.
ref_name(Name, Name).

mint_type_decl_specs(Decls0, Decls, Rows) :-
    maplist(mint_type_decl(Decls0), Decls0, DeclLists, RowLists),
    append(DeclLists, Decls),
    append(RowLists, Unsorted),
    sort(Unsorted, Rows).

mint_type_decl(AllDecls, type_decl(OwnerName, Specs0), Decls, Rows) :-
    !,
    semantic_owner_id(AllDecls, OwnerName, Owner),
    mint_type_decl_specs_(AllDecls, Owner, Specs0, Specs, ExtraDecls, Rows),
    Decls = [type_decl(OwnerName, Specs) | ExtraDecls].
mint_type_decl(_, Decl, [Decl], []).

mint_type_decl_specs_(_, _, [], [], [], []).
mint_type_decl_specs_(Decls, Owner, [col(Column, Type0) | Rest],
                     [col(Column, Type) | More], ExtraDecls, Rows) :-
    ( type_contains_anonymous(Type0)
    -> anonymous_mint(Decls, Owner, [Column], Type0, Type,
                      ColumnDecls, ColumnRows)
    ;  Type = Type0, ColumnDecls = [], ColumnRows = []
    ),
    mint_type_decl_specs_(Decls, Owner, Rest, More, RestDecls, RestRows),
    append(ColumnDecls, RestDecls, ExtraDecls),
    append(ColumnRows, RestRows, Rows).

% ═══ minting walk ════════════════════════════════════════════════════════════
%
% anonymous_mint(+Decls, +Owner, +Path, +Type0, -Type, -ExtraDecls, -Rows)
%
% Type0 is the specialized literal type; Type is the materialized type with
% anonymous sub-terms replaced by their generated names.  Owner is the root
% owning relation's semantic id and stays fixed throughout the descent; Path
% grows with member names and wrapper/application ordinals.  ExtraDecls are the
% materialized declarations for this site and every nested site; Rows are the
% semantic rows (identity + declaration + link) for the same.

anonymous_mint(Decls, Owner, Path, product_type(Fields0), Type,
               ExtraDecls, Rows) :-
    !,
    anonymous_mint_product(Decls, Owner, Path, product_type(Fields0), Fields0,
                           Type, ExtraDecls, Rows).
anonymous_mint(Decls, Owner, Path, arrow_type(Inputs, Output), Type,
               ExtraDecls, Rows) :-
    !,
    append(Inputs, [field(return, Output)], Fields),
    anonymous_mint_product(Decls, Owner, Path, arrow_type(Inputs, Output),
                           Fields, Type, ExtraDecls, Rows).

anonymous_mint(Decls, Owner, Path, sum_type(Variants0), Type,
               ExtraDecls, Rows) :-
    !,
    check_anonymous_cycle(Owner, Path, sum_type(Variants0)),
    mint_variants(Decls, Owner, Path, Variants0, Variants, VariantDecls,
                  VariantRows),
    anonymous_type_name(Owner, Path, sum_type(Variants0), GeneratedName),
    variants_to_terms(Variants, Terms),
    semantic_decl_id_anon(Owner, GeneratedName, enum, GeneratedId),
    owner_module_hash(Owner, ModuleHash),
    Id = anonymous(Owner, Path, sum_type(Variants0)),
    ExtraDecls = [ enum_decl(GeneratedName, Terms),
                   semantic_decl_module(enum, GeneratedName, ModuleHash),
                   anonymous_generated_decl(GeneratedName)
                 | VariantDecls ],
    Rows = [ anonymous(Owner, Path, sum_type(Variants0)),
             declaration(GeneratedId, root, GeneratedName, enum, compile_time),
             derived_from(GeneratedId, Id)
           | VariantRows ],
    Type = GeneratedName.
anonymous_mint(Decls, Owner, Path, annotated_type(Type0, Applications),
               annotated_type(Type, Applications), ExtraDecls, Rows) :-
    !,
    anonymous_mint(Decls, Owner, Path, Type0, Type, ExtraDecls, Rows).
% A wrapper (list/option/json_list) or a generic application: descend into each
% argument with its ordinal appended to the path.
anonymous_mint(Decls, Owner, Path, Type0, Type, ExtraDecls, Rows) :-
    compound(Type0),
    Type0 =.. [Functor | Args0],
    mint_arguments(Decls, Owner, Path, 1, Args0, Args, ArgDecls, ArgRows),
    !,
    ( Args == Args0
    -> Type = Type0, ExtraDecls = [], Rows = []
    ; Type =.. [Functor | Args],
      ExtraDecls = ArgDecls,
      Rows = ArgRows
    ).
anonymous_mint(_, _, _, Type, Type, [], []).

anonymous_mint_product(Decls, Owner, Path, IdentityShape, Fields0, Type,
                       ExtraDecls, Rows) :-
    check_anonymous_cycle(Owner, Path, product_type(Fields0)),
    mint_fields(Decls, Owner, Path, Fields0, Fields, FieldDecls, FieldRows),
    anonymous_type_name(Owner, Path, IdentityShape, GeneratedName),
    cols_from_fields(Fields, Specs),
    length(Specs, Arity),
    findall(col_type(GeneratedName/Arity, Column, ColumnType),
            member(col(Column, ColumnType), Specs),
            ColumnDecls),
    semantic_decl_id_anon(Owner, GeneratedName, relation, GeneratedId),
    owner_module_hash(Owner, ModuleHash),
    Id = anonymous(Owner, Path, IdentityShape),
    append([ type_decl(GeneratedName, Specs),
             semantic_decl_module(relation, GeneratedName, ModuleHash),
             anonymous_generated_decl(GeneratedName)
           | ColumnDecls ], FieldDecls, ExtraDecls),
    Rows = [ anonymous(Owner, Path, IdentityShape),
             declaration(GeneratedId, root, GeneratedName, relation,
                         materialized),
             derived_from(GeneratedId, Id)
           | FieldRows ],
    Type = GeneratedName.

mint_arguments(_, _, _, _, [], [], [], []).
mint_arguments(Decls, Owner, Path, Ordinal, [Arg0 | Rest],
               [Arg | More], Decls1, Rows) :-
    ( type_contains_anonymous(Arg0)
    -> append(Path, [Ordinal], ChildPath),
       anonymous_mint(Decls, Owner, ChildPath, Arg0, Arg, ArgDecls, ArgRows)
    ; Arg = Arg0, ArgDecls = [], ArgRows = []
    ),
    Next is Ordinal + 1,
    mint_arguments(Decls, Owner, Path, Next, Rest, More, RestDecls, RestRows),
    append(ArgDecls, RestDecls, Decls1),
    append(ArgRows, RestRows, Rows).

mint_fields(_, _, _, [], [], [], []).
mint_fields(Decls, Owner, Path, [field(Name, Type0) | Rest],
            [field(Name, Type) | More], Decls1, Rows) :-
    ( type_contains_anonymous(Type0)
    -> append(Path, [Name], ChildPath),
       anonymous_mint(Decls, Owner, ChildPath, Type0, Type,
                      FieldDecls, FieldRows)
    ; Type = Type0, FieldDecls = [], FieldRows = []
    ),
    mint_fields(Decls, Owner, Path, Rest, More, RestDecls, RestRows),
    append(FieldDecls, RestDecls, Decls1),
    append(FieldRows, RestRows, Rows).

mint_variants(_, _, _, [], [], [], []).
mint_variants(Decls, Owner, Path, [variant(Name, Fields0) | Rest],
              [variant(Name, Fields) | More], Decls1, Rows) :-
    append(Path, [Name], ChildPath),
    mint_fields(Decls, Owner, ChildPath, Fields0, Fields, FieldDecls,
                FieldRows),
    mint_variants(Decls, Owner, Path, Rest, More, RestDecls, RestRows),
    append(FieldDecls, RestDecls, Decls1),
    append(FieldRows, RestRows, Rows).

% ═══ shape and name ══════════════════════════════════════════════════════════

cols_from_fields(Fields, Specs) :-
    maplist([field(Name, Type), col(Name, Type)]>>true, Fields, Specs).

% A sum is materialized as an enum_decl; its variants reuse the ordinary enum
% surface `Name(field: type)` (the `:`/2 field spelling enum lowering already
% consumes), so the anonymous `field/2` terms convert back to `:`/2.
variants_to_terms(Variants, Terms) :-
    maplist(variant_to_term, Variants, TermList),
    variants_join(TermList, Terms).

variant_to_term(variant(Name, Fields), Term) :-
    maplist([field(FName, FType), FName:FType]>>true, Fields, FieldTerms),
    Term =.. [Name | FieldTerms].

variants_join([Term], Term).
variants_join([Term | Rest], (Term ; More)) :-
    variants_join(Rest, More).

% Deterministic diagnostic name: owner stem + path stem + a 16-hex digest of
% the canonical identity encoding.  Unrelated declarations cannot change the
% identity, so they cannot change the name either.
anonymous_type_name(Owner, Path, Shape, Name) :-
    Id = anonymous(Owner, Path, Shape),
    semantic_type_id_text(Id, Digest),
    sub_atom(Digest, 0, 16, _, Short),
    id_kind_name(Owner, _, OwnerName),
    path_stem(Path, PathStem),
    atomic_list_concat(['__anon', OwnerName, PathStem, Short], '_', Name).

path_stem(Path, Stem) :-
    maplist(path_component_stem, Path, Components),
    atomic_list_concat(Components, '_', Stem).

path_component_stem(N, Stem) :-
    integer(N), !,
    number_codes(N, Codes),
    atom_codes(Stem, Codes).
path_component_stem(A, A).

% ═══ helpers ═════════════════════════════════════════════════════════════════

type_contains_anonymous(product_type(_)) :- !.
type_contains_anonymous(sum_type(_)) :- !.
type_contains_anonymous(arrow_type(_, _)) :- !.
type_contains_anonymous(annotated_type(Type, _)) :-
    !,
    type_contains_anonymous(Type).
type_contains_anonymous(Term) :-
    compound(Term),
    compound_name_arguments(Term, _, Args),
    member(Arg, Args),
    type_contains_anonymous(Arg).

% An unguarded cycle is refused. `option(T)` and `list(T)` provide an existing
% storage boundary, so an owner reference below either wrapper is accepted.
check_anonymous_cycle(Owner, Path, Shape) :-
    id_kind_name(Owner, _, OwnerName),
    ( unguarded_shape_references_name(Shape, OwnerName)
    -> throw(unsupported_construct(anonymous_type_cycle(Owner, Path)))
    ; true
    ).

unguarded_shape_references_name(Shape, Name) :-
    nonvar(Shape),
    ( Shape == Name
    -> true
    ; Shape = option(_)
    -> false
    ; Shape = list(_)
    -> false
    ; compound(Shape),
      compound_name_arguments(Shape, _, Args),
      member(Arg, Args),
      unguarded_shape_references_name(Arg, Name)
    ).

semantic_owner_id(Decls, OwnerName, Owner) :-
    ( member(semantic_decl_module(Kind, OwnerName, ModuleHash), Decls)
    -> true
    ; member(semantic_type_rows(Rows), Decls),
      member(declaration(Owner, root, OwnerName, Kind, _), Rows)
    -> true
    ; ( member(enum_decl(OwnerName, _), Decls) -> Kind = enum ; Kind = relation ),
      ModuleHash = local
    ),
    ( var(Owner) -> decl_id(ModuleHash, Kind, OwnerName, Owner) ; true ).

owner_module_hash(named(ModuleHash, _, _), ModuleHash).

semantic_decl_id_anon(Owner, GeneratedName, Kind, Id) :-
    owner_module_hash(Owner, ModuleHash),
    decl_id(ModuleHash, Kind, GeneratedName, Id).

merge_anonymous_type_rows([], Decls, Decls).
merge_anonymous_type_rows(Rows, Decls0, Decls) :-
    ( memberchk(semantic_type_rows(_), Decls0)
    -> maplist(merge_one_anonymous_type_rows(Rows), Decls0, Decls)
    ; append(Decls0, [semantic_type_rows(Rows)], Decls)
    ).

merge_one_anonymous_type_rows(AnonRows, semantic_type_rows(Rows0),
                              semantic_type_rows(Rows)) :-
    !,
    append(Rows0, AnonRows, Unsorted),
    sort(Unsorted, Rows).
merge_one_anonymous_type_rows(_, Decl, Decl).

% Generic validation rows retain source shapes only in anonymous/3 identity
% witnesses; all generic argument references use materialized declaration ids.
rewrite_anonymous_semantic_rows(Decls0, Decls) :-
    ( member(semantic_type_rows(_), Decls0)
    -> maplist(rewrite_one_anonymous_semantic_rows(Decls0), Decls0, Decls)
    ; Decls = Decls0
    ).

rewrite_one_anonymous_semantic_rows(Decls,
                                    semantic_type_rows(Rows0),
                                    semantic_type_rows(Rows)) :-
    !,
    maplist(rewrite_anonymous_semantic_row(Decls), Rows0, Rewritten),
    sort(Rewritten, Rows).
rewrite_one_anonymous_semantic_rows(_, Decl, Decl).

rewrite_anonymous_semantic_row(_, anonymous(Owner, Path, Shape),
                               anonymous(Owner, Path, Shape)) :- !.
rewrite_anonymous_semantic_row(Decls, Row0, Row) :-
    rewrite_anonymous_semantic_term(Decls, Row0, Row).

rewrite_anonymous_semantic_term(_, anonymous(Owner, Path, Shape),
                                anonymous(Owner, Path, Shape)) :- !.
rewrite_anonymous_semantic_term(Decls, anonymous_placeholder(Shape), GeneratedId) :-
    !,
    anonymous_generated_id(Decls, Shape, GeneratedId).
rewrite_anonymous_semantic_term(Decls, type_named(Shape),
                                type_declaration(GeneratedId)) :-
    anonymous_type_shape(Shape),
    !,
    anonymous_generated_id(Decls, Shape, GeneratedId).
rewrite_anonymous_semantic_term(Decls, type_ref(named(Shape)),
                                type_ref(declaration(GeneratedId))) :-
    anonymous_type_shape(Shape),
    !,
    anonymous_generated_id(Decls, Shape, GeneratedId).
rewrite_anonymous_semantic_term(Decls, Type, GeneratedId) :-
    anonymous_type_shape(Type),
    !,
    anonymous_generated_id(Decls, Type, GeneratedId).
rewrite_anonymous_semantic_term(Decls, annotated_type(Type0, Applications),
                                annotated_type(Type, Applications)) :-
    !,
    rewrite_anonymous_semantic_term(Decls, Type0, Type).
rewrite_anonymous_semantic_term(_, Term, Term) :- atomic(Term), !.
rewrite_anonymous_semantic_term(Decls, Term0, Term) :-
    Term0 =.. [Functor | Args0],
    maplist(rewrite_anonymous_semantic_term(Decls), Args0, Args),
    Term =.. [Functor | Args].

anonymous_type_shape(product_type(_)).
anonymous_type_shape(sum_type(_)).
anonymous_type_shape(arrow_type(_, _)).

anonymous_generated_id(Decls, Shape, GeneratedId) :-
    setof(Id,
          Owner^Path^GeneratedName^Rows^
          ( member(semantic_type_rows(Rows), Decls),
            member(anonymous(Owner, Path, Shape), Rows),
            member(derived_from(Id, anonymous(Owner, Path, Shape)), Rows),
            member(declaration(Id, root, GeneratedName, _, _), Rows) ),
          [GeneratedId | _]).

% ═══ role path (shared with schema_member_roles) ═════════════════════════════

% A member whose authored type is an anonymous product/sum carries the
% anonymous_owner role; the path is the minted site path.
anonymous_owner_path(anonymous(_, Path, _), Path) :- !.
