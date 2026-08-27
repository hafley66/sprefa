% 0_type_plane.pl : shared relation-reference schema utilities.
%
% There is one relation model and one checker. A rel column naming another rel
% means:
%
%   target(__id, key..., fields...)
%   parent(..., target_id INTEGER, ...)
%
% Nested ingress is normalized into ordinary target arrivals followed by the
% parent arrival. Target membership is public and queryable at that tick.
% SQLite stores typed target columns and the parent integer endpoint. Canonical
% JSON exists only at the wire/render boundary.
%
% ── the decl ─────────────────────────────────────────────────────────────────
%
%   type_decl(TypeName, [col(Column, Type), ...])
%
% `type_decl/2` is a legacy compiler IR record produced from a `rel`
% declaration referenced in column position. It contributes schema metadata
% to the same rel. It does not create a language type, hidden dictionary,
% second table family, or second checker.
%
%   rel span(start: int, end: int).
%   rel finding(path: text, at: span).
%
% A column schema is `int`, `text`, transient `json`, or a target rel name.
% Target identity follows key(...), with full-row identity as the unkeyed
% fallback. `__id` is the dense physical mate used by parent edges.

:- module(type_plane,
          [ type_definitions/2,
            type_definition/4,
            declared_type_name/2,
            relation_id_type/2,
            column_storage/3,
            type_wrapper/2,
            unwrapped_column_type/2,
            column_element_type_name/2,
            type_topological_order/2,
            type_cycle_witness/2,
            type_shape_error/4,
            world_row_shape_violation/3,
            canonicalize_world_rows/3,
            normalize_relation_reference_rows/3,
            type_canonical_json/4,
            type_field_values/4,
            type_ref_columns/3,
            canonical_json_text/2,
            js_float_text/2,
            % Exported for sweep.pl's schedule JSON writer.
            escape_json_codes/2,
            json_object_value/2,
            relation_columns_and_types/5,
            relation_value_shape/3,
            relation_value_object/4,
            relation_value_term/4
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('body', [json_canon/2]).

% ── the type table ───────────────────────────────────────────────────────────

% Every type_decl/2 a program declares, in declaration order.
type_definitions(Decls, Types) :-
    findall(type_def(Name, Columns, ColumnTypes),
            ( member(type_decl(Name, Specs), Decls),
              findall(Column, member(col(Column, _), Specs), Columns),
              findall(Type, member(col(_, Type), Specs), ColumnTypes) ),
            Types).

type_definition(Types, Name, Columns, ColumnTypes) :-
    memberchk(type_def(Name, Columns, ColumnTypes), Types).

declared_type_name(Types, Name) :- memberchk(type_def(Name, _, _), Types).

% `Revision.id` is retained as `id(Revision)` after qualified-type resolution.
% It denotes the target relation's existing SQLite endpoint, not a relation
% value and not a request to intern or follow that row.
relation_id_type(id(Name), Name) :- atom(Name).

% The storage kind of a declared column type. `json` stores TEXT and keeps
% the inline json1 path (SLOT-JSON1-FATE: untyped json only, never a cache of
% a typed value).
column_storage(_, int,  int) :- !.
column_storage(_, text, text) :- !.
column_storage(_, bytes, bytes) :- !.
% `json` is its own STORAGE KIND, not an alias for text. The kind is what the
% decode lowering dispatches on (lower.pl json_decode_source/4): the brace
% pattern's lowering is a function of the SOURCE COLUMN'S DECLARED TYPE, never
% of the pattern, so a declared struct becomes a dictionary join and a `json`
% column becomes json1 SQL. Collapsing json into text here erased exactly the
% bit that choice reads. Physical storage is still TEXT (lower.pl column_def/3
% adds the json_valid CHECK); `jsonb` is NOT portable across the two SQLite
% builds this project already runs (3.43.2 CLI rejects it, @libsql 3.45.1
% accepts) so the stored bytes stay TEXT.
column_storage(_, json, json) :- !.
% ruling list_spelling = list_of_type ("list(text) seems easy enough").
%
% THE VERDICT THIS EXECUTES (json_syntax lab §3): json is the array CARRIER and
% `json_list(T)` is a TYPED VIEW over it. Relational element storage is not a list,
% it is a rel with an index column, declared as such. The lab graded three
% options on five axes with a receipt per cell; the carrier wins content
% identity (the canonical text IS the id, and it is already the log contract),
% retraction (one column in one row, zero cascade), aggregate heads
% (json_group_array is a native aggregate), the tick-log contract (storage IS
% the contract, render is identity) and 0/1/many (`[]` is a value and is not
% absence). Measured: one 1000-element list is 1 row as a carrier, 1001 as
% indexed rows, 1000 as cons cells, and all three render byte-identically.
%
% T ranges over a CLOSED four-element set, so there is no type variable, no
% unification and no instantiation -- that is what lets json_list(T) be the only
% parametric type without dragging generics into the checker. The whole
% measured delta is these four clauses: the storage row, the element guard, and
% the two named unsupported constructs below.
%
% NAMED, not silently absent: SQLite can enforce ARRAY-NESS as a column CHECK
% (json_valid(c) AND json_type(c) = 'array', verified) and CANNOT enforce the
% ELEMENT type -- CHECK constraints prohibit subqueries and json_each is a
% table function. Element typing is a checker/emitted-guard obligation. Today
% the storage kind collapses to `json`, so neither guard is emitted; the
% array-ness CHECK needs json_list(T) to survive as its own kind all the way to
% lower.pl:column_def/3, which widens every place that matches on `json`.
column_storage(Types, json_list(Element), json_list(Element)) :-
    !,
    (   list_element_type(Types, Element)
    ->  true
    ;   declared_type_name(Types, Element)
    ->  throw(unsupported_construct(list_of_relation_refs(Element)))
    ;   throw(unsupported_construct(list_element_not_scalar(Element)))
    ).

% Stores the minted entity's INTEGER id (lower.pl:column_def/4). The element
% lands in the member rel's `value` column, so every column type is admissible.
column_storage(Types, list(Element), list(Element)) :-
    !,
    column_storage(Types, Element, _).

column_storage(_, bool, bool) :- !.
column_storage(_, float, float) :- !.
column_storage(Types, id(Name), idref(Name)) :-
    !,
    ( declared_type_name(Types, Name)
    -> true
    ; throw(unsupported_construct(relation_id_target_unknown(Name)))
    ).
column_storage(Types, Name, ref(Name)) :- declared_type_name(Types, Name), !.
column_storage(_, Name, _) :-
    throw(unsupported_construct(column_type_unknown(Name))).

% A list element is a value, and one of the four scalars, a json document, or
% a nested list whose own element is admissible. A relation ref is refused
% separately from any other non-scalar because the reason differs: ids in a
% list would enter the tick log, breaking the print-values-never-ids ruling,
% while a nested list and a json element are what the widening admits.
list_element_type(_, int).
list_element_type(_, text).
list_element_type(_, bool).
list_element_type(_, float).
list_element_type(_, json).
list_element_type(Types, json_list(Inner)) :- list_element_type(Types, Inner).

% Where a wrapper puts a rel element: `value` in a column of the rel it mints
% (list_flavor_artifacts/2), `endpoint` as an id (companion_rel_decls/4).
type_wrapper(option, endpoint).
type_wrapper(list, value).
type_wrapper(list_entity_dense_sequence, value).
type_wrapper(list_interned_set, value).
type_wrapper(list_entity_linked_sequence, value).

% Every type expression a spelling carries, itself first. Each step descends
% into a strict subterm of a finite ground term, so the walk reaches the leaf.
unwrapped_column_type(Type, Type).
unwrapped_column_type(Type, Inner) :-
    compound(Type),
    Type =.. [Wrapper, Element],
    type_wrapper(Wrapper, _),
    unwrapped_column_type(Element, Inner).

% The rel name a spelling puts in COLUMN position, hence the name that needs a
% type_decl/2 mirror: the bare name, or the element of a value-storing wrapper.
column_element_type_name(Name, Name) :- atom(Name).
column_element_type_name(Type, Name) :-
    unwrapped_column_type(Type, id(Name)),
    atom(Name),
    !.
column_element_type_name(Type, Name) :-
    unwrapped_column_type(Type, Inner),
    Inner =.. [Wrapper, Name],
    type_wrapper(Wrapper, value),
    atom(Name).

% The ref columns of one type, as Column-ChildType pairs.
type_ref_columns(Types, Name, RefColumns) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    findall(Column-ChildType,
            ( nth1(Position, Columns, Column),
              nth1(Position, ColumnTypes, ChildType),
              declared_type_name(Types, ChildType) ),
            RefColumns).

% ── one relation's columns, whichever door asks ──────────────────────────────
% A rel reached in COLUMN position carries its shape in a type_decl/2; a rel
% reached only as an ordinary relation carries it in col_type/3 rows. Both
% spellings describe the same rel, and a caller walking a rule body cannot know
% in advance which one a given atom has, so this answers from either.
relation_columns_and_types(_, Types, Name/Arity, Columns, ColumnTypes) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    length(Columns, Arity),
    !.
relation_columns_and_types(Decls, _, Ref, Columns, ColumnTypes) :-
    findall(Column-Type, member(col_type(Ref, Column, Type), Decls), Pairs),
    pairs_keys_values(Pairs, Columns, ColumnTypes).

% ── a relation value written as a term ───────────────────────────────────────
%
% `file(repo(Name), fpath(Path))` in a rule head or body argument, over a
% column declared `file`. This is the SURFACE spelling of a relation value; the
% stored/graded spelling is the canonical obj(...) below, which is also what a
% world arrival produces. Keeping the two apart is the whole of the depth fix:
% every door turns the term into the object, and no door stores the term.
%
% The shape test is exact -- right name, right arity. A name that matches no
% declared type, or the right name at the wrong arity, is NOT a relation value
% and is a named unsupported construct at the door
% (0_program_check.pl relation_pattern_not_a_relation_value), never a value
% that quietly stores as something else.
relation_value_shape(Types, TypeName, Value) :-
    nonvar(Value),
    compound(Value),
    functor(Value, TypeName, Arity),
    type_definition(Types, TypeName, Columns, _),
    length(Columns, Arity).

% A relation-valued column supplies the missing context for an object literal.
% The named relation-term spelling remains accepted, while `{field: value}`
% and the canonical obj/1 spelling are lowered to the same positional term
% before the dictionary rewrite runs. This is deliberately exact: a partial or
% extra object cannot be assigned to a product owner by field resemblance.
relation_value_term(Types, TypeName, Value, Term) :-
    relation_value_shape(Types, TypeName, Value),
    !,
    Term = Value.
relation_value_term(Types, TypeName, Value, Term) :-
    relation_object_fields(Types, TypeName, Value, Fields),
    Term =.. [TypeName | Fields].

relation_object_fields(Types, TypeName, Value, Fields) :-
    object_pairs_preserving_variables(Value, Pairs),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    pairs_keys(Pairs, PairKeys0),
    sort(PairKeys0, PairKeys),
    length(Pairs, PairCount),
    length(PairKeys, KeyCount),
    PairCount =:= KeyCount,
    sort(Columns, ColumnKeys),
    PairKeys == ColumnKeys,
    maplist(relation_object_field(Types, Pairs), Columns, ColumnTypes,
            Fields).

relation_object_field(Types, Pairs, Column, ColumnType, Field) :-
    memberchk(Column-Value, Pairs),
    ( declared_type_name(Types, ColumnType)
    -> relation_value_term(Types, ColumnType, Value, Field)
    ;  Field = Value
    ).

object_pairs_preserving_variables(obj(Pairs0), Pairs) :-
    is_list(Pairs0),
    maplist(object_pair, Pairs0, Pairs1),
    keysort(Pairs1, Pairs).
object_pairs_preserving_variables(Value, Pairs) :-
    nonvar(Value),
    Value = '{}'(Raw),
    conjunction_pairs(Raw, RawPairs),
    maplist(object_pair, RawPairs, Pairs1),
    keysort(Pairs1, Pairs).

object_pair(Key-Value, Key-Value) :- !.
object_pair(Key:Value, Key-Value).

conjunction_pairs((Left, Right), Pairs) :- !,
    conjunction_pairs(Left, LeftPairs),
    conjunction_pairs(Right, RightPairs),
    append(LeftPairs, RightPairs, Pairs).
conjunction_pairs(Pair, [Pair]).

% The canonical object a relation-value term denotes, recursively, with keys
% sorted the way every other object in the system is sorted. Unbound arguments
% pass through as themselves, so a PATTERN (`file(_, fpath(Path))`) becomes a
% pattern object that unifies against a stored value at any depth.
relation_value_object(Types, TypeName, Value, obj(Sorted)) :-
    relation_value_shape(Types, TypeName, Value),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    Value =.. [_ | Args],
    maplist(relation_field_object(Types), Columns, ColumnTypes, Args, Pairs),
    keysort(Pairs, Sorted).
relation_value_object(Types, TypeName, Value, obj(Sorted)) :-
    relation_object_fields(Types, TypeName, Value, Fields),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    maplist(relation_field_object(Types), Columns, ColumnTypes, Fields,
            Pairs),
    keysort(Pairs, Sorted).

relation_field_object(Types, Column, ChildType, Arg, Column-Out) :-
    (   relation_value_object(Types, ChildType, Arg, Object)
    ->  Out = Object
    ;   Out = Arg
    ).

% ── ordering: children before parents ────────────────────────────────────────
% Interning is post-order, so every statement family below (DDL, intern,
% render) walks the type graph in this order. It FAILS on a cycle rather than
% looping; type_cycle_witness/2 is what names the offender.

type_topological_order(Types, Ordered) :-
    findall(Name, member(type_def(Name, _, _), Types), Names),
    topological_rounds(Types, Names, [], Ordered).

topological_rounds(_, [], Acc, Acc) :- !.
topological_rounds(Types, Pending, Acc, Ordered) :-
    include(children_all_placed(Types, Acc), Pending, Ready),
    Ready \== [],
    subtract(Pending, Ready, Rest),
    append(Acc, Ready, Acc1),
    topological_rounds(Types, Rest, Acc1, Ordered).

children_all_placed(Types, Placed, Name) :-
    type_ref_columns(Types, Name, RefColumns),
    forall(member(_-ChildType, RefColumns), memberchk(ChildType, Placed)).

% A type that never becomes ready is on a cycle. Content ids cannot express a
% cyclic reference graph -- a parent's key is computed FROM its children's
% keys (verdict: interned_graph_is_a_dag) -- so this is a unsupported construct, not a
% capability gap. The entity plane, which permits cycles, is out of this arc.
type_cycle_witness(Types, Names) :-
    findall(Name, member(type_def(Name, _, _), Types), AllNames),
    AllNames \== [],
    \+ type_topological_order(Types, _),
    settled_prefix(Types, AllNames, [], Settled),
    subtract(AllNames, Settled, Names),
    Names \== [].

settled_prefix(Types, Pending, Acc, Settled) :-
    include(children_all_placed(Types, Acc), Pending, Ready),
    (   Ready == []
    ->  Settled = Acc
    ;   subtract(Pending, Ready, Rest),
        append(Acc, Ready, Acc1),
        settled_prefix(Types, Rest, Acc1, Settled)
    ).

% ── arrival shape ────────────────────────────────────────────────────────────
% SLOT-ARRIVAL-MALFORMED, decided: `type_arrival_shape_mismatch`. A world row
% whose value does not match the declared struct shape is a NAMED unsupported construct at
% the boundary, never a silently stored blob and never a NULL column. The
% reasons are enumerated so the message says which key, not just "bad shape".
%
% A struct-typed column accepts a JSON OBJECT value only. A plain Prolog
% compound term (`fresh(tag_w1, body1)`) is NOT an accepted spelling of a
% struct value: the oracle's tick log renders such a term as canonical PROLOG
% text (ticklog.pl term_text/2) while a struct renders as canonical JSON, so
% accepting the functor form would silently change the graded bytes of a
% value that already has a meaning. That is an oracle semantics question, not
% a storage one; SLOT-TERM-STRUCT names it and this arc leaves the functor
% form on the untyped inline path exactly as it is today.

%
% SLOT-ARRIVAL-CANONICAL-ORDER, RULED 2026-07-29 (user: "we know the types
% order so we can induce it", rulings.pl struct_arrival_key_order): arrival
% key order is INSIGNIFICANT. The declaration names the field set, so the
% oracle canonicalizes a struct-typed arrival value to sorted-key obj/1 form
% at the world boundary (canonicalize_world_rows/3, called from run_program
% right after check_world_shapes) instead of refusing. The divergence the old
% keys_not_sorted unsupported construct guarded -- oracle term identity treating
% obj([start-1,end-2]) and obj([end-2,start-1]) as two rows while the emitted
% side interns both to one -- is unreachable the other way now: both
% spellings become ONE canonical term before any store or Set sees them. The
% emitted runtime already canonicalized at intern; nothing changes there.

type_shape_error(Types, TypeName, Value, Reason) :-
    (   json_object_value(Value, Pairs)
    ->  type_definition(Types, TypeName, Columns, ColumnTypes),
        (   member(Column, Columns), \+ memberchk(Column-_, Pairs)
        ->  Reason = missing_key(TypeName, Column)
        ;   member(Key-_, Pairs), \+ memberchk(Key, Columns)
        ->  Reason = unknown_key(TypeName, Key)
        ;   nth1(Position, Columns, Column),
            nth1(Position, ColumnTypes, ChildType),
            memberchk(Column-ChildValue, Pairs),
            field_shape_error(Types, TypeName, Column, ChildType, ChildValue, Reason)
        )
    ;   Reason = not_an_object(TypeName, Value)
    ).

field_shape_error(Types, _, _, ChildType, ChildValue, Reason) :-
    declared_type_name(Types, ChildType), !,
    type_shape_error(Types, ChildType, ChildValue, Reason).
field_shape_error(_, TypeName, Column, int, ChildValue, Reason) :-
    \+ integer(ChildValue), !,
    Reason = field_not_int(TypeName, Column, ChildValue).
field_shape_error(_, TypeName, Column, bool, ChildValue, Reason) :-
    \+ bool_value(ChildValue), !,
    Reason = field_not_bool(TypeName, Column, ChildValue).
field_shape_error(_, TypeName, Column, float, ChildValue, Reason) :-
    \+ finite_float(ChildValue), !,
    Reason = field_not_finite_float(TypeName, Column, ChildValue).
field_shape_error(_, TypeName, Column, text, ChildValue, Reason) :-
    \+ atomic(ChildValue), !,
    Reason = field_not_text(TypeName, Column, ChildValue).

% Both surface spellings of a JSON object reach here: the raw braces literal
% a fixture or a host row writes, and the canonical obj(SortedPairs) body.pl
% produces. Sorted pairs, always, so a caller may index by column name.
json_object_value(Value, Pairs) :-
    nonvar(Value),
    (   Value == '{}'
    ->  Pairs = []
    ;   Value = obj(RawPairs)
    ->  is_list(RawPairs), keysort(RawPairs, Pairs)
    ;   Value = {}(_)
    ->  json_canon(Value, obj(Pairs))
    ).

% ── world-row canonicalization (struct_arrival_key_order ruling) ────────────
% Rewrite every struct-typed column value in a world-row list to the ONE
% canonical spelling: obj(SortedPairs), recursively through nested declared
% types, untyped inner json normalized by json_canon. Runs once at load
% (run_program, after check_world_shapes passes), so every store, Set
% membership, and tick-log render downstream sees a single term per value.
% Rows of partially-typed refs pass through untouched -- the same named
% crack as the shape check below, same reason.
%
% It ALSO applies SQLite's REAL affinity (ruling type_gate_widening): an
% integer landing in a `float` column becomes a float here, once, so the
% reference engine stores exactly what a REAL column stores and every later
% read -- sum, avg, comparison, tick log -- sees a double on both doors. This
% is why the Types == [] short circuit is gone: the widening is a property of
% the column type, not of the value plane, and a program with no struct types
% at all still has float columns.
canonicalize_world_rows(Decls, Rows0, Rows) :-
    type_definitions(Decls, Types),
    maplist(canonicalize_signed_row(Decls, Types), Rows0, Rows).

% A nested relation row on ingress is shorthand for two ordinary arrivals:
% the target row first, then the parent row carrying its resolved endpoint.
% The oracle stores logical terms rather than dense ids, so it retains the
% parent object value while adding the same public target memberships and
% clocks that the SQLite door applies before endpoint rewriting.
normalize_relation_reference_rows(Decls, Rows0, Rows) :-
    type_definitions(Decls, Types),
    (   Types == []
    ->  Rows = Rows0
    ;   type_topological_order(Types, TypeOrder),
        findall(Type-Target,
                ( member(SignedRow, Rows0),
                  bare_row(SignedRow, Row),
                  relation_reference_target(Decls, Types, Row, Type, Target) ),
                TargetPairs),
        ordered_target_rows(TypeOrder, TargetPairs, Targets0),
        maplist(reference_target_wrapper(Rows0), Targets0, Targets1),
        exclude(reference_target_already_arriving(Rows0), Targets1, Targets),
        append(Targets, Rows0, Rows)
    ).

relation_reference_target(Decls, Types, Row, TypeName, Target) :-
    compound(Row),
    Row =.. [Name | Values],
    length(Values, Arity),
    relation_columns_and_types(Decls, Types, Name/Arity, _, ColumnTypes),
    nth1(Position, ColumnTypes, TypeName),
    declared_type_name(Types, TypeName),
    nth1(Position, Values, Value),
    type_field_values(Types, TypeName, Value, Fields),
    Target =.. [TypeName | Fields].
relation_reference_target(Decls, Types, Row, TypeName, Target) :-
    compound(Row),
    Row =.. [Name | Values],
    length(Values, Arity),
    relation_columns_and_types(Decls, Types, Name/Arity, _, ColumnTypes),
    nth1(Position, ColumnTypes, ChildType),
    declared_type_name(Types, ChildType),
    nth1(Position, Values, Value),
    type_field_values(Types, ChildType, Value, Fields),
    Child =.. [ChildType | Fields],
    relation_reference_target(Decls, Types, Child, TypeName, Target).

ordered_target_rows([], _, []).
ordered_target_rows([Type | Types], TargetPairs, Targets) :-
    findall(Target, member(Type-Target, TargetPairs), RawTargets),
    dedupe_preserving_order(RawTargets, TypeTargets),
    ordered_target_rows(Types, TargetPairs, Rest),
    append(TypeTargets, Rest, Targets).

dedupe_preserving_order([], []).
dedupe_preserving_order([Item | Items], [Item | Unique]) :-
    exclude(==(Item), Items, Rest),
    dedupe_preserving_order(Rest, Unique).

reference_target_wrapper(Rows, Target, Wrapped) :-
    ( member(Signed, Rows), ( Signed = +(_) ; Signed = -(_) ) )
    -> Wrapped = +Target
    ;  Wrapped = Target.

reference_target_already_arriving(Rows, Target) :-
    Target \= +(_),
    memberchk(Target, Rows).

canonicalize_signed_row(Decls, Types, +(Row0), +(Row)) :- !,
    canonicalize_row(Decls, Types, Row0, Row).
canonicalize_signed_row(Decls, Types, -(Row0), -(Row)) :- !,
    canonicalize_row(Decls, Types, Row0, Row).
canonicalize_signed_row(Decls, Types, Row0, Row) :-
    canonicalize_row(Decls, Types, Row0, Row).

canonicalize_row(Decls, Types, Row0, Row) :-
    (   compound(Row0),
        functor(Row0, Name, Arity),
        ref_column_names(Decls, Name/Arity, Arity, Columns)
    ->  Row0 =.. [Name | Values0],
        maplist(canonicalize_column(Decls, Types, Name/Arity), Columns, Values0, Values),
        Row =.. [Name | Values]
    ;   Row = Row0
    ).

canonicalize_column(Decls, Types, Ref, Column, Value0, Value) :-
    (   nonvar(Value0),
        memberchk(col_type(Ref, Column, TypeName), Decls),
        declared_type_name(Types, TypeName)
    ->  canonical_struct_value(Types, TypeName, Value0, Value)
    ;   nonvar(Value0),
        memberchk(col_type(Ref, Column, float), Decls),
        integer(Value0)
    ->  Value is float(Value0)
    ;   Value = Value0
    ).

canonical_struct_value(Types, TypeName, Value0, Value) :-
    % Rule-authored object construction is first made positional by
    % relation_pattern.pl.  The positional child can itself be an anonymous
    % product, so run the same relation-value normalization here before the
    % ordinary JSON/object path.  This keeps nested authored and ingress
    % values on the same canonical object representation.
    relation_value_object(Types, TypeName, Value0, Value),
    !.
canonical_struct_value(Types, TypeName, Value0, obj(Pairs)) :-
    json_object_value(Value0, Pairs0),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    maplist(canonical_field_value(Types, Columns, ColumnTypes), Pairs0, Pairs),
    !.
canonical_struct_value(_, _, Value, Value).

canonical_field_value(Types, Columns, ColumnTypes, Key-Value0, Key-Value) :-
    (   nth1(Position, Columns, Key),
        nth1(Position, ColumnTypes, ChildType),
        declared_type_name(Types, ChildType)
    ->  canonical_struct_value(Types, ChildType, Value0, Value)
    ;   nonvar(Value0), ( Value0 = {}(_) ; is_list(Value0) )
    ->  json_canon(Value0, Value)
    ;   Value = Value0
    ).

% Every world row a program seeds or a schedule delivers, checked against the
% declared shape of the column it lands in. Rows is a flat list of signed or
% bare row terms; the sign wrapper is stripped here so both doors hand over
% whatever they already have.
%
% This is a decl-driven unsupported construct, not an execution change: a row that passes
% behaves exactly as it did before the type existed. It runs where the data
% is static (fixture Initial + Schedule on both doors); the emitted runtime
% repeats it at intern time, where the data is not.
world_row_shape_violation(Decls, Rows, mismatch(Ref, Column, TypeName, Reason)) :-
    type_definitions(Decls, Types),
    member(SignedRow, Rows),
    bare_row(SignedRow, Row),
    compound(Row),
    functor(Row, Name, Arity),
    Ref = Name/Arity,
    row_column_violation(Decls, Types, Ref, Arity, Row, Column, TypeName, Reason),
    !.

% Two passes over one row, in this order.
%
% 1. The WIDE-INTEGER pass, which is decl-INDEPENDENT (ruling wide_int_fate =
%    refuse_everywhere_with_todo). An integer outside the IEEE-754 safe range
%    reaches SQLite through whatever column it was written to, and the
%    JavaScript side of every emitted program answers three different things
%    for it depending on which column type it crossed and which emitter mode
%    ran. So the value is refused wherever it appears, including inside a json
%    document, and including a rel with no colon types at all -- which is why
%    this pass does not go through ref_column_names/4.
%
%    ONE exception, and it is the other ruling: a `float` column. REAL affinity
%    widens an integer to a double BEFORE anything asks how big it was, so
%    there is no integer left to be out of range -- 9007199254740993 at a float
%    column is the double 9007199254740992.0, approximate by construction, on
%    both doors. Skipping the column here is also what keeps the two doors
%    equal: the emitted guard cannot tell integer 1e20 from float 1.0e20 and
%    reads the declaration for the answer, exactly as this does.
% 2. The DECLARED-TYPE pass, which needs the column's declared type and so
%    needs full col_type/3 coverage.
row_column_violation(Decls, _, Ref, Arity, Row, Column, none, int_out_of_range(Value)) :-
    between(1, Arity, Position),
    arg(Position, Row, Argument),
    nonvar(Argument),
    wide_integer_witness(Argument, Value),
    position_column_name(Decls, Ref, Position, Column),
    \+ memberchk(col_type(Ref, Column, float), Decls).
row_column_violation(Decls, Types, Ref, Arity, Row, Column, TypeName, Reason) :-
    ref_column_names(Decls, Ref, Arity, Columns),
    nth1(Position, Columns, Column),
    memberchk(col_type(Ref, Column, TypeName), Decls),
    arg(Position, Row, Value),
    nonvar(Value),
    column_value_shape_error(Types, TypeName, Value, Reason).

% The declared column name where the decl carries one, the bare position
% otherwise. The wide-int pass runs on undeclared rels too, and a unsupported construct that
% cannot say WHERE is worse than one that says "column 2".
position_column_name(Decls, Ref, Position, Column) :-
    (   findall(Name, member(col_type(Ref, Name, _), Decls), Names),
        nth1(Position, Names, Named)
    ->  Column = Named
    ;   Column = Position
    ).

% An integer beyond Number.MAX_SAFE_INTEGER, anywhere in a world value:
% bare, inside a json object, or inside a json array. Floats are NOT
% inspected: a float is already inexact and crosses the seam as a double on
% both sides.
%
% TODO(bigint): this is the REFUSAL half of wide_int_fate. The other half --
% carrying arbitrary-precision integers end to end -- needs a bigint column
% type, an @libsql intMode decision, and a tick-log encoding for values JSON
% numbers cannot hold. Ruled out for now (user 2026-07-31: "not finance yet");
% the matching seam-side TODO is in v6/tsv2/runtime/rows.ts.
wide_integer_witness(Value, Value) :-
    integer(Value),
    ( Value < -9007199254740991 ; Value > 9007199254740991 ),
    !.
wide_integer_witness(obj(Pairs), Witness) :-
    is_list(Pairs), !,
    member(_-Child, Pairs),
    wide_integer_witness(Child, Witness).
wide_integer_witness(List, Witness) :-
    is_list(List), !,
    member(Child, List),
    wide_integer_witness(Child, Witness).
wide_integer_witness({}(Body), Witness) :-
    !,
    json_canon({}(Body), Canonical),
    Canonical = obj(Pairs),
    member(_-Child, Pairs),
    wide_integer_witness(Child, Witness).

% RULING type_gate_widening = arrival_gate_all_types_all_positions
% (user 2026-07-31, "widen yes, do what sql would do"). Every declared column
% type answers here, not just the three the float/avg arc reached, and the
% answer for a type mix is SQLite's own affinity rule:
%
%   int column    integer, or a REAL with an exact integer value (INTEGER
%                 affinity converts a lossless real). Anything else refused.
%   float column  finite float, or an INTEGER, which WIDENS -- REAL affinity
%                 converts an integer to a double. This is the one direction
%                 the ruling names explicitly.
%   bool column   the two bool literals, nothing else. A bool column is a
%                 CHECK-constrained integer in storage, so a non-boolean was
%                 previously either dropped by the CHECK or coerced by it,
%                 depending on which door and which head position; the ruling
%                 kills both silences by name.
%   text column   an atom or a string. A number at a TEXT column used to be
%                 the biggest divergence family in the matrix (the engine kept
%                 the number, the emitter's TEXT affinity stringified it).
%
% A json column is NOT checked here: its arrival value has already been through
% compile/scripts/0_json_arrival.pl, the shared reader both .dl6 doors use,
% which already refuses a non-document. A list column IS checked here (the
% array-ness and element arm below), because array-ness is exactly what the
% reader does not verify. The wide-int pass above reaches inside both.
column_value_shape_error(_, bool, Value, field_not_bool(Value)) :-
    \+ bool_value(Value), !.
column_value_shape_error(_, float, Value, field_not_finite_float(Value)) :-
    \+ float_column_value(Value), !.
column_value_shape_error(_, int, Value, int_out_of_range(Value)) :-
    integer(Value),
    ( Value < -9007199254740991
    ; Value > 9007199254740991
    ),
    !.
column_value_shape_error(_, int, Value, field_not_int(Value)) :-
    \+ int_column_value(Value), !.
column_value_shape_error(_, id(_), Value, field_not_int(Value)) :-
    \+ int_column_value(Value), !.
column_value_shape_error(_, text, Value, field_not_text(Value)) :-
    \+ text_column_value(Value), !.
% Bytes receive their concrete tagged arrival representation with target
% lowering. Until then the type is usable in declarations and derived rows,
% while an authored world value stops here instead of entering BLOB storage
% through SQLite's affinity coercions.
column_value_shape_error(_, bytes, Value, field_not_bytes(Value)) :-
    \+ bytes_value(Value), !.
% A list column's arrival value is a json array whose elements must satisfy
% the element type's own shape check. Runs here even though a json column is
% not checked: the shared arrival reader only verifies the value is a
% document, and a CHECK constraint cannot reach into elements (json_each is a
% table function and CHECK prohibits subqueries).
column_value_shape_error(_, json_list(_), Value, field_not_array(Value)) :-
    \+ is_list(Value), !.
column_value_shape_error(Types, json_list(Element), Value, list_element_shape(Index, Reason)) :-
    nth1(Index, Value, Elem),
    column_value_shape_error(Types, Element, Elem, Reason),
    !.
column_value_shape_error(Types, TypeName, Value, Reason) :-
    declared_type_name(Types, TypeName),
    type_shape_error(Types, TypeName, Value, Reason).

int_column_value(Value) :- integer(Value), !.
int_column_value(Value) :- finite_float(Value), Value =:= truncate(Value).

float_column_value(Value) :- finite_float(Value), !.
float_column_value(Value) :- integer(Value).

text_column_value(Value) :- atom(Value), !.
text_column_value(Value) :- string(Value).

bool_value(bool_lit(true)).
bool_value(bool_lit(false)).

bytes_value(bytes(Base64)) :- atom(Base64), !.
bytes_value(bytes(Base64)) :- string(Base64).

finite_float(Value) :-
    float(Value),
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]).

bare_row(+(Row), Row) :- !.
bare_row(-(Row), Row) :- !.
bare_row(Row, Row).

% Column names for a ref, in declared order, and ONLY when the col_type/3
% entries cover every argument position. analyze.pl:rel_columns/5 is the
% general answer, but it needs the surface variable Bindings, which the
% oracle door does not have -- so this door reads position from declaration
% order and refuses to guess when the declaration is partial.
%
% NAMED CRACK: a rel whose columns are only PARTIALLY typed gets no arrival
% shape check here at all (position would be mis-located), so a malformed
% struct value in such a rel is caught later, at intern time in the emitted
% runtime, rather than at load. Fully-typed decls are what the corpus writes
% and what the printer synthesizes; the partial case is silent by choice, not
% by accident.
ref_column_names(Decls, Ref, Arity, Columns) :-
    findall(Column, member(col_type(Ref, Column, _), Decls), Columns),
    length(Columns, Arity).

% ── decomposition and rendering ──────────────────────────────────────────────

% The declared columns' values, in declared order. Child struct values come
% back as their own object terms; the caller recurses.
type_field_values(Types, TypeName, Value, FieldValues) :-
    json_object_value(Value, Pairs),
    type_definition(Types, TypeName, Columns, _),
    findall(FieldValue,
            ( member(Column, Columns), memberchk(Column-FieldValue, Pairs) ),
            FieldValues),
    length(Columns, Width),
    length(FieldValues, Width).

% The memoized rendering, computed once per interned value. A parent's text
% is one concat over its children's texts, which is the whole reason Edge 1
% needs no recursion at read: by the time a parent interns, every child row
% already carries its own finished __rendered.
% Object keys come out SORTED, never in declared column order: the ruled
% cross-target encoding is sorted-keys-no-whitespace, and declaration order
% is a positional-program fact that must not leak into a logical value.
type_canonical_json(Types, TypeName, Value, Text) :-
    json_object_value(Value, Pairs),
    type_definition(Types, TypeName, Columns, ColumnTypes),
    msort(Columns, SortedColumns),
    findall(Entry,
            ( member(Column, SortedColumns),
              nth1(Position, Columns, Column),
              nth1(Position, ColumnTypes, ChildType),
              memberchk(Column-ChildValue, Pairs),
              type_field_json(Types, ChildType, ChildValue, ChildText),
              json_string_text(Column, KeyText),
              atomic_list_concat([KeyText, ':', ChildText], Entry) ),
            Entries),
    atomic_list_concat(Entries, ',', Inner),
    atomic_list_concat(['{', Inner, '}'], Text).

type_field_json(Types, ChildType, ChildValue, Text) :-
    declared_type_name(Types, ChildType), !,
    type_canonical_json(Types, ChildType, ChildValue, Text).
type_field_json(_, _, ChildValue, Text) :- canonical_json_text(ChildValue, Text).

% ── canonical JSON text ──────────────────────────────────────────────────────
% Sorted object keys, no whitespace: the cross-target log contract the
% json_ticklog_encoding ruling fixed. Deliberately a clause-for-clause mirror
% of conformance/ticklog.pl:value_json/2 rather than a call into it --
% ticklog.pl is a SCRIPT (`:- ensure_loaded(go)`), not a module, so the
% compiler cannot import it without dragging the whole oracle in. The
% agreement is pinned by test, and ultimately by the byte-identical tick-log
% grade itself.

canonical_json_text(Value, Text) :- integer(Value), !, format(atom(Text), '~w', [Value]).
canonical_json_text(bool_lit(Boolean), Text) :- !, format(atom(Text), '~w', [Boolean]).
canonical_json_text(none, null) :- !.
canonical_json_text(Value, null) :- Value == json_null, !.
canonical_json_text(Value, Text) :- float(Value), !, finite_float_json(Value, Text).
canonical_json_text(Value, Text) :- nonvar(Value), Value = json_object(Pairs), !,
    keysort(Pairs, Sorted),
    findall(Entry,
            ( member(Key-Raw, Sorted),
              json_string_text(Key, KeyText),
              canonical_json_text(Raw, RawText),
              atomic_list_concat([KeyText, ':', RawText], Entry) ),
            Entries),
    atomic_list_concat(Entries, ',', Inner),
    atomic_list_concat(['{', Inner, '}'], Text).
canonical_json_text(Value, Text) :- nonvar(Value), Value = json_array(Values), !,
    maplist(canonical_json_text, Values, ElementTexts),
    atomic_list_concat(ElementTexts, ',', Inner),
    atomic_list_concat(['[', Inner, ']'], Text).
canonical_json_text(Value, Text) :- json_object_value(Value, Pairs), !,
    findall(Entry,
            ( member(Key-Raw, Pairs),
              json_string_text(Key, KeyText),
              canonical_json_text(Raw, RawText),
              atomic_list_concat([KeyText, ':', RawText], Entry) ),
            Entries),
    atomic_list_concat(Entries, ',', Inner),
    atomic_list_concat(['{', Inner, '}'], Text).
canonical_json_text(Value, Text) :- is_list(Value), !,
    maplist(canonical_json_text, Value, ElementTexts),
    atomic_list_concat(ElementTexts, ',', Inner),
    atomic_list_concat(['[', Inner, ']'], Text).
canonical_json_text(Value, Text) :- compound(Value), !,
    term_json_text(Value, Raw), json_string_text(Raw, Text).
canonical_json_text(Value, Text) :- json_string_text(Value, Text).

finite_float_json(Value, Text) :-
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]),
    ( Value =:= 0.0
    -> Text = '0'
    ; js_float_text(Value, Text)
    ).

% SWI-Prolog's ~h is shortest round-trip, but its exponent threshold differs
% from ECMAScript Number::toString. Rewrite that same shortest digit sequence
% using JavaScript's fixed range [1e-6, 1e21), preserving the exact binary64
% value represented by the source float.
js_float_text(Value, Text) :-
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal]),
    format(atom(Raw), '~h', [Value]),
    ( Value < 0.0 -> Sign = '-', sub_atom(Raw, 1, _, 0, UnsignedRaw)
    ; Sign = '', UnsignedRaw = Raw
    ),
    ( split_float_exponent(UnsignedRaw, Mantissa, Exponent)
    -> mantissa_digits(Mantissa, Digits, IntegerDigits),
       decimal_position(IntegerDigits, Exponent, DecimalPosition),
       js_float_digits(Sign, Digits, DecimalPosition, Text)
    ; normalize_fixed_float_atom(UnsignedRaw, UnsignedText),
      atom_concat(Sign, UnsignedText, Text)
    ).

split_float_exponent(Raw, Mantissa, Exponent) :-
    ( sub_atom(Raw, Before, 1, After, 'e')
    ; sub_atom(Raw, Before, 1, After, 'E')
    ),
    sub_atom(Raw, 0, Before, _, Mantissa),
    Start is Before + 1,
    sub_atom(Raw, Start, After, 0, ExponentText),
    atom_number(ExponentText, Exponent).

mantissa_digits(Mantissa, Digits, IntegerDigits) :-
    atom_chars(Mantissa, Chars),
    append(IntegerChars, FractionWithDot, Chars),
    FractionWithDot = ['.' | FractionChars],
    append(IntegerChars, FractionChars, DigitsChars),
    Digits = DigitsChars,
    length(IntegerChars, IntegerDigits).

decimal_position(IntegerDigits, Exponent, Position) :-
    Position is IntegerDigits + Exponent.

js_float_digits(Sign, Digits, DecimalPosition, Text) :-
    length(Digits, DigitCount),
    ( DecimalPosition > 0,
      DecimalPosition =< 21
    -> fixed_positive_digits(Digits, DecimalPosition, UnsignedText),
       atom_concat(Sign, UnsignedText, Text)
    ; DecimalPosition =< 0,
      DecimalPosition >= -5
    -> fixed_fraction_digits(Digits, DecimalPosition, UnsignedText),
       atom_concat(Sign, UnsignedText, Text)
    ; scientific_digits(Digits, DecimalPosition, UnsignedText),
      atom_concat(Sign, UnsignedText, Text)
    ),
    DigitCount > 0.

fixed_positive_digits(Digits, DecimalPosition, Text) :-
    length(Prefix, DecimalPosition),
    append(Prefix, Suffix, Digits),
    atom_chars(PrefixAtom, Prefix),
    ( Suffix == []
    -> Text = PrefixAtom
    ; strip_trailing_zero_chars(Suffix, TrimmedSuffix),
      ( TrimmedSuffix == []
      -> Text = PrefixAtom
      ; atom_chars(TrimmedAtom, TrimmedSuffix),
        format(atom(Text), '~w.~w', [PrefixAtom, TrimmedAtom])
      )
    ).
fixed_positive_digits(Digits, DecimalPosition, Text) :-
    DecimalPosition >= 0,
    length(Digits, DigitCount),
    DecimalPosition >= DigitCount,
    atom_chars(DigitsAtom, Digits),
    ZeroCount is DecimalPosition - DigitCount,
    zero_chars(ZeroCount, Zeros),
    atom_chars(ZeroAtom, Zeros),
    atom_concat(DigitsAtom, ZeroAtom, Text).

fixed_fraction_digits(Digits, DecimalPosition, Text) :-
    ZeroCount is -DecimalPosition,
    zero_chars(ZeroCount, LeadingZeros),
    append(['0', '.'], LeadingZeros, PrefixChars),
    append(PrefixChars, Digits, AllChars),
    strip_trailing_zero_chars(AllChars, TrimmedChars),
    atom_chars(Text, TrimmedChars).

scientific_digits(Digits, DecimalPosition, Text) :-
    Digits = [First | Rest],
    strip_trailing_zero_chars(Rest, TrimmedRest),
    atom_chars(FirstAtom, [First]),
    ( TrimmedRest == []
    -> MantissaText = FirstAtom
    ; atom_chars(RestAtom, TrimmedRest),
      format(atom(MantissaText), '~w.~w', [FirstAtom, RestAtom])
    ),
    Exponent is DecimalPosition - 1,
    ( Exponent >= 0 -> ExponentText = '+' ; ExponentText = '' ),
    format(atom(Text), '~we~w~w', [MantissaText, ExponentText, Exponent]).

normalize_fixed_float_atom(Raw, Text) :-
    ( sub_atom(Raw, Before, 2, 0, '.0')
    -> sub_atom(Raw, 0, Before, _, Text)
    ; Text = Raw
    ).

strip_trailing_zero_chars(Chars, Trimmed) :-
    reverse(Chars, Reversed),
    drop_leading_zeros(Reversed, ReversedTrimmed),
    reverse(ReversedTrimmed, Trimmed).

drop_leading_zeros(['0' | Rest], Trimmed) :- !,
    drop_leading_zeros(Rest, Trimmed).
drop_leading_zeros(Chars, Chars).

zero_chars(0, []) :- !.
zero_chars(Count, ['0' | Rest]) :-
    NextCount is Count - 1,
    zero_chars(NextCount, Rest).

normalize_float_json_atom(Raw, Text) :-
    ( sub_atom(Raw, Before, 2, After, '.0'),
      ( After =:= 0
      ; Start is Before + 2,
        sub_atom(Raw, Start, _, 0, Exponent),
        ( sub_atom(Exponent, 0, 1, _, 'e')
        ; sub_atom(Exponent, 0, 1, _, 'E') ) )
    -> sub_atom(Raw, 0, Before, _, Prefix),
       Start2 is Before + 2,
       sub_atom(Raw, Start2, After, 0, Suffix),
       atom_concat(Prefix, Suffix, Text)
    ; Text = Raw
    ).

term_json_text(Value, Text) :- atomic(Value), !, format(atom(Text), '~w', [Value]).
term_json_text(Value, Text) :- compound(Value), !,
    Value =.. [Name | Args],
    maplist(term_json_text, Args, ArgTexts),
    atomic_list_concat(ArgTexts, ',', Inner),
    format(atom(Text), '~w(~w)', [Name, Inner]).

json_string_text(Value, Text) :-
    format(atom(Raw), '~w', [Value]),
    atom_codes(Raw, Codes),
    escape_json_codes(Codes, EscapedCodes),
    atom_codes(Escaped, EscapedCodes),
    format(atom(Text), '"~w"', [Escaped]).

% THE ESCAPE SET IS `JSON.stringify`'s, exactly, because the tsv2 door IS
% JSON.stringify and the tick log is graded by BYTE DIFF: two spellings of the
% same character are two different logs. ECMA-262 QuoteJSONString escapes
% " \ \b \f \n \r \t by name and every remaining code below 0x20 as
% \uXXXX, four LOWERCASE hex digits.
%
% What this replaced, and why nothing caught it (json_flex lab, 2026-07-30):
% the hex arm was `format(atom(H), '\\u~`0t~16r~4|', [Code])`. `~4|` is a
% COLUMN stop measured from the start of the atom and `\u` already occupies
% two columns, so the escape came out with TWO hex digits -- code 12 rendered
% `\u0c`, then the next source character was appended to it and the result
% was `\u0cb`, which is not JSON at all. `\b`, `\f` and `\r` all went down
% that arm. Zero fixtures in the 209-fixture corpus carried a control
% character, so an oracle tick log that JSON.parse rejects passed every
% byte-diff gate this project has. Fixture
% json_string_control_escapes_are_valid_json is the fail-first receipt: the
% sweep's own reader threw `Bad Unicode escape in JSON at position 45`.
escape_json_codes([], []).
escape_json_codes([Code | Rest], Out) :-
    json_escaped_codes(Code, Escaped),
    escape_json_codes(Rest, RestOut),
    append(Escaped, RestOut, Out).

json_escaped_codes(0'", [0'\\, 0'"]) :- !.
json_escaped_codes(0'\\, [0'\\, 0'\\]) :- !.
json_escaped_codes(8,  [0'\\, 0'b]) :- !.
json_escaped_codes(12, [0'\\, 0'f]) :- !.
json_escaped_codes(10, [0'\\, 0'n]) :- !.
json_escaped_codes(13, [0'\\, 0'r]) :- !.
json_escaped_codes(9,  [0'\\, 0't]) :- !.
json_escaped_codes(Code, Escaped) :-
    Code < 32, !,
    % `~|` opens a column stop HERE (after the `\u` already written) and
    % `~4+` closes it four columns later, so the zero fill is over the hex
    % digits alone. `~4|` -- the absolute stop the broken version used --
    % counts the `\u` inside its own budget and leaves two digits.
    format(atom(HexAtom), '\\u~|~`0t~16r~4+', [Code]),
    atom_codes(HexAtom, Escaped).
json_escaped_codes(Code, [Code]).
