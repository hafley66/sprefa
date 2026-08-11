% Generic expansion closes schema templates before enum expansion.
%
% The artifact table uses typed records.  `lower_artifacts/2` is the one
% boundary where a template's schema records become the program's Decl terms.
% Round one emits declarations only.  Rules remain author-written.
:- module(generic_expand,
          [ expand_generic_in_context/3,
            expand_generic_program/2,
            expand_generic_program_raw/2,
            canonical_type_name/2,
            canonical_type_encoding/2,
            generic_artifact_order/3,
            generated_generic_name/1
          ]).

:- use_module(library(apply)).
:- use_module(library(crypto)).
:- use_module(library(lists)).
:- use_module('0_option_expand', [expand_option_decls/2]).

expand_generic_in_context(_Context, Program, Expanded) :-
    expand_generic_program(Program, Expanded).

expand_generic_program(prog(Decls0, Rules), prog(Decls, Rules)) :-
    generic_fixpoint(Decls0, Instances, WithMintedDecls),
    validate_generated_name_collisions(Decls0, Rules, Instances),
    replace_generic_types(WithMintedDecls, Instances, RewrittenDecls),
    generic_artifact_order(Instances, RewrittenDecls, CanonicalDecls),
    expand_option_decls(CanonicalDecls, Decls).

% Executable comparison arm, written as a second path so the template and
% replacement logic cannot drift apart from the wired entry above.
expand_generic_program_raw(prog(Decls0, Rules), prog(Decls, Rules)) :-
    generic_fixpoint(Decls0, Instances, WithMintedDecls),
    validate_generated_name_collisions(Decls0, Rules, Instances),
    replace_generic_types(WithMintedDecls, Instances, RewrittenDecls),
    generic_artifact_order(Instances, RewrittenDecls, CanonicalDecls),
    expand_option_decls(CanonicalDecls, Decls).

% A worklist is represented by the canonical sorted instance list.  The four
% list constructors are term-door-only lab constructors.  No parser spelling
% is claimed here.

% Discovery fixes over minted decls (an outer member's value column is itself
% a list), so each pass mints the not-yet-lowered instances and re-scans.
generic_fixpoint(SourceDecls, Instances, AllDecls) :-
    check_interned_set_rel_elements(SourceDecls),
    generic_fixpoint_(SourceDecls, [], AllDecls, Instances).

% A rel element does not belong inside the interned-set value dictionary: the
% rel row already interns it (the rel id IS the interned id). Named here rather
% than forced through the redundant content-addressed dictionary.
check_interned_set_rel_elements(SourceDecls) :-
    declared_type_names(SourceDecls, Names),
    (   member(col_type(_, _, list_interned_set(Element)), SourceDecls),
        memberchk(Element, Names)
    ->  throw(unsupported_construct(list_interned_set_relation_element(Element)))
    ;   true
    ).

declared_type_names(Decls, Names) :-
    findall(Name, member(type_decl(Name, _), Decls), Names).

generic_fixpoint_(Decls, MintedSoFar, AllDecls, Instances) :-
    generic_type_instances(Decls, AllFound),
    subtract(AllFound, MintedSoFar, NewInstances),
    maplist(template_artifacts, NewInstances, ArtifactLists),
    append(ArtifactLists, Artifacts),
    lower_artifacts(Artifacts, NewDecls),
    append(MintedSoFar, NewInstances, AllMintedInstances),
    append(Decls, NewDecls, NextDecls),
    ( NewInstances == []
    -> AllDecls = Decls,
       Instances = AllMintedInstances
    ; generic_fixpoint_(NextDecls, AllMintedInstances, AllDecls, Instances)
    ).

generic_type_instances(Decls, Instances) :-
    findall(Type, ( member(col_type(_, _, Type), Decls), generic_type(Type) ),
            Found),
    maplist(check_ground_generic, Found),
    findall(Instance,
            ( member(Type, Found), generic_dependency(Type, Instance) ),
            FoundInstances),
    sort(FoundInstances, Instances).

generic_type(list(_)).
generic_type(list_entity_dense_sequence(_)).
generic_type(list_interned_set(_)).
generic_type(list_entity_linked_sequence(_)).
generic_type(option(Type)) :- contains_list_flavor(Type).

contains_list_flavor(Type) :- list_flavor(Type).
contains_list_flavor(option(Type)) :- contains_list_flavor(Type).

generic_dependency(option(Type), Instance) :-
    generic_dependency(Type, Instance).
generic_dependency(list(Element), list(Element)).
generic_dependency(Type, Type) :- named_list_flavor(Type).

list_flavor(list(_)).
list_flavor(Type) :- named_list_flavor(Type).

named_list_flavor(list_entity_dense_sequence(_)).
named_list_flavor(list_interned_set(_)).
named_list_flavor(list_entity_linked_sequence(_)).

check_ground_generic(Type) :-
    ( ground(Type) -> true
    ; throw(unsupported_construct(generic_type_not_ground(Type)))
    ).

% Typed artifact vocabulary.  Additional templates add records here and the
% lowering relation remains the sole place coupled to Decl syntax.
template_artifacts(Type, Artifacts) :-
    list_flavor(Type),
    !,
    list_flavor_artifacts(Type, Artifacts).
template_artifacts(option(_), []).

list_flavor_artifacts(list(Element), Artifacts) :-
    flavor_ref(list(Element), list, Entity),
    flavor_ref(list(Element), member, Member),
    Artifacts = [ artifact(decl(col_type(Entity, id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Member, list_id, int))),
                  artifact(decl(col_type(Member, idx, int))),
                  artifact(decl(col_type(Member, value, Element))),
                  artifact(decl(keyed(Member, [1, 2]))) ].
list_flavor_artifacts(list_entity_dense_sequence(Element), Artifacts) :-
    flavor_ref(list_entity_dense_sequence(Element), list, Entity),
    flavor_ref(list_entity_dense_sequence(Element), member, Member),
    flavor_ref(list_entity_dense_sequence(Element), owner, Owner),
    flavor_ref(list_entity_dense_sequence(Element), refcount, Refcount),
    Artifacts = [ artifact(decl(col_type(Entity, id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Member, list_id, int))),
                  artifact(decl(col_type(Member, idx, int))),
                  artifact(decl(col_type(Member, value, Element))),
                  artifact(decl(keyed(Member, [1, 2]))),
                  artifact(decl(col_type(Owner, owner_id, int))),
                  artifact(decl(col_type(Owner, list_id, int))),
                  artifact(decl(keyed(Owner, [1, 2]))),
                  artifact(decl(col_type(Refcount, list_id, int))),
                  artifact(decl(col_type(Refcount, count, int))),
                  artifact(decl(keyed(Refcount, [1]))) ].
list_flavor_artifacts(list_interned_set(Element), Artifacts) :-
    flavor_ref(list_interned_set(Element), list, Entity),
    flavor_ref(list_interned_set(Element), value, Value),
    flavor_ref(list_interned_set(Element), member, Member),
    Artifacts = [ artifact(decl(col_type(Entity, content_id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Value, id, int))),
                  artifact(decl(col_type(Value, value, Element))),
                  artifact(decl(keyed(Value, [2]))),
                  artifact(decl(col_type(Member, content_id, int))),
                  artifact(decl(col_type(Member, value_id, int))),
                  artifact(decl(keyed(Member, [1, 2]))) ].
list_flavor_artifacts(list_entity_linked_sequence(Element), Artifacts) :-
    flavor_ref(list_entity_linked_sequence(Element), list, Entity),
    flavor_ref(list_entity_linked_sequence(Element), member, Member),
    flavor_ref(list_entity_linked_sequence(Element), link, Link),
    Artifacts = [ artifact(decl(col_type(Entity, id, int))),
                  artifact(decl(keyed(Entity, [1]))),
                  artifact(decl(col_type(Member, member_id, int))),
                  artifact(decl(col_type(Member, list_id, int))),
                  artifact(decl(col_type(Member, value, Element))),
                  artifact(decl(keyed(Member, [1]))),
                  artifact(decl(col_type(Link, before_member_id, int))),
                  artifact(decl(col_type(Link, after_member_id, int))),
                  artifact(decl(keyed(Link, [1, 2]))) ].

flavor_ref(Type, list, Name/1) :- canonical_type_name(Type, Name).
flavor_ref(Type, Suffix, Name/Arity) :-
    Suffix \== list,
    canonical_type_name(Type, Base),
    atomic_list_concat([Base, Suffix], '__', Name),
    flavor_ref_arity(Suffix, Arity).

flavor_ref_arity(member, 3).
flavor_ref_arity(owner, 2).
flavor_ref_arity(refcount, 2).
flavor_ref_arity(value, 2).
flavor_ref_arity(link, 2).

lower_artifacts([], []).
lower_artifacts([artifact(decl(Decl)) | Rest], [Decl | Decls]) :-
    lower_artifacts(Rest, Decls).

% Author declarations retain source order; generated declarations follow them.
generic_artifact_order([], Decls, Decls).
generic_artifact_order([_ | _], Decls, Ordered) :-
    partition(minted_decl, Decls, Minted, Author),
    append(Author, Minted, Ordered).

minted_decl(col_type(Ref/_, _, _)) :- generated_generic_name(Ref).
minted_decl(keyed(Ref/_, _)) :- generated_generic_name(Ref).

validate_generated_name_collisions(Decls, Rules, Instances) :-
    findall(Name,
            ( member(Type, Instances), template_generated_name(Type, Name) ),
            GeneratedNames),
    sort(GeneratedNames, UniqueNames),
    length(GeneratedNames, GeneratedCount),
    length(UniqueNames, GeneratedCount),
    findall(Name, author_decl_or_rule_name(Decls, Rules, Name), AuthorNames),
    ( member(Name, GeneratedNames), memberchk(Name, AuthorNames)
    -> throw(unsupported_construct(generic_generated_name_collision(Name)))
    ; true
    ).

template_generated_name(Type, Name) :-
    list_flavor_suffix(Type, Suffix),
    flavor_ref(Type, Suffix, Name/_).

list_flavor_suffix(list(_), list).
list_flavor_suffix(list(_), member).
list_flavor_suffix(list_entity_dense_sequence(_), list).
list_flavor_suffix(list_entity_dense_sequence(_), member).
list_flavor_suffix(list_entity_dense_sequence(_), owner).
list_flavor_suffix(list_entity_dense_sequence(_), refcount).
list_flavor_suffix(list_interned_set(_), list).
list_flavor_suffix(list_interned_set(_), value).
list_flavor_suffix(list_interned_set(_), member).
list_flavor_suffix(list_entity_linked_sequence(_), list).
list_flavor_suffix(list_entity_linked_sequence(_), member).
list_flavor_suffix(list_entity_linked_sequence(_), link).

author_decl_or_rule_name(Decls, _, Name) :-
    member(Decl, Decls),
    plain_decl_name(Decl, Name).
author_decl_or_rule_name(_, Rules, Name) :-
    member(Rule, Rules),
    Rule =.. [_, Head | _],
    compound(Head), functor(Head, Name, _).

plain_decl_name(col_type(Name/_, _, _), Name).
plain_decl_name(keyed(Name/_, _), Name).
plain_decl_name(kind(Name/_, _), Name).
plain_decl_name(keep(Name/_, _), Name).

replace_generic_types([], _, []).
replace_generic_types([col_type(Ref, Column, Type0) | Rest], Instances,
                       [col_type(Ref, Column, Type) | Rewritten]) :-
    !,
    replace_generic_type(Type0, Instances, Type),
    replace_generic_types(Rest, Instances, Rewritten).
replace_generic_types([Decl | Rest], Instances, [Decl | Rewritten]) :-
    replace_generic_types(Rest, Instances, Rewritten).

replace_generic_type(option(Type0), Instances, option(Name)) :-
    !,
    replace_option_element(Type0, Instances, Name).
% A bare list column IS its list entity's id, so the column type lowers to the
% integer id exactly like every companion `_id` column the flavors already emit.
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
readable_stem(Type, Stem) :-
    Type =.. [Constructor | Args],
    maplist(readable_stem, Args, Stems),
    atomic_list_concat([Constructor | Stems], '_', Stem).

generated_generic_name(Name) :-
    atom(Name), sub_atom(Name, 0, _, _, '__gen_').
