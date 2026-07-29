% 0_type_plane.pl : the declared value plane -- referenced relation values,
% their storage
% kind per column, their topological order, their canonical JSON rendering,
% and the shape check a world arrival must satisfy.
%
% RULED 2026-07-29 (rulings.pl compound_storage = struct_as_rows): a declared
% struct value is a rel row referenced by content id. A parent column stores
% the ref, never an inline blob; destructuring becomes joins. This file is the
% ONE place that says what a relation-valued column is, consumed by BOTH doors (the
% oracle's engine.pl and the compiler's analyze/lower/emit) exactly the way
% 0_enum_expand.pl and 0_match_expand.pl are.
%
% ── the decl ─────────────────────────────────────────────────────────────────
%
%   type_decl(TypeName, [col(Column, Type), ...])
%
% The surface has one declaration word. A `rel` referenced from another
% relation's column type position is normalized to this internal declaration.
%
%   rel span(start: int, end: int).
%   rel finding(path: text, at: span).
%
% The lab (plans/2026-07-28-types-as-rels-verdict.md, THE SHORTHAND TABLE)
% derives a struct decl as `rel` + `key(every content column)`, and that IS
% the semantics this file implements -- the dictionary table is exactly a
% keyed set table over the content columns. What it is NOT is a program rel,
% and that is forced by the arc header's Edge 2: dictionary rows must not
% reach the boundary, because the oracle holds real terms and has no
% dictionary at all, so any dictionary rel in the tick log would be a rel the
% oracle can never produce. The current normalization classifies the
% referenced declaration as a value relation. Its dictionary stays outside
% the program relation boundary.
%
% A column type is `int`, `text`, `json` (the untyped-json escape hatch, per
% SLOT-JSON1-FATE: json1 stays as the representation of UNTYPED json ONLY) or
% the name of a declared type, which is the ref storage kind.
%
% ── the two identities, kept in separate columns ─────────────────────────────
%
% Per the round-2 surrogate-mate ruling: semantic identity and storage key are
% different jobs and never share a column.
%
%   __semantic  the content key. Derived from the type name plus the value's
%               canonical content, with every CHILD represented by the child's
%               own semantic key -- never by the child's dense id, which is
%               build-order dependent (verdict:
%               parent_hash_from_dense_would_be_order_dependent).
%   __id        the dense integer storage mate. SQLite assigns it. It never
%               crosses the logical boundary, so re-interning a released value
%               under a different dense id is allowed and unobservable.
%   __rendered  the memoized canonical JSON text, computed ONCE at intern
%               time (arc header Edge 1). Values are immutable and children
%               intern before parents, so a parent's rendering is one concat
%               over child renderings and there is no recursion at read.
%
% __semantic here is the canonical text itself, prefixed by the type name,
% not a digest of it. That is a deliberate, priced choice: @libsql/client
% 0.17.4 registers NO user-defined functions at all (measured, verdict
% plans/2026-07-29-sqlite-udf-graft-verdict.md, "all four candidate method
% names undefined") and SQLite ships no built-in hash, so a digest would have
% to be computed outside SQL and threaded through every set-based statement.
% Full canonical text is injective on values for a fixed type -- strictly
% stronger than a hash, with no collision case to reason about -- and costs
% storage, not correctness. SLOT-SEMANTIC-DIGEST names the swap for the day a
% UDF seam exists.

:- module(type_plane,
          [ type_definitions/2,
            type_definition/4,
            declared_type_name/2,
            column_storage/3,
            type_topological_order/2,
            type_cycle_witness/2,
            type_shape_error/4,
            world_row_shape_violation/3,
            canonicalize_world_rows/3,
            type_canonical_json/4,
            type_field_values/4,
            type_ref_columns/3,
            canonical_json_text/2,
            json_object_value/2
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('conformance/body', [json_canon/2]).

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

% The storage kind of a declared column type. `json` stores TEXT and keeps
% the inline json1 path (SLOT-JSON1-FATE: untyped json only, never a cache of
% a typed value).
column_storage(_, int,  int) :- !.
column_storage(_, text, text) :- !.
column_storage(_, json, text) :- !.
column_storage(Types, Name, ref(Name)) :- declared_type_name(Types, Name), !.
column_storage(_, Name, _) :-
    throw(unsupported_construct(column_type_unknown(Name))).

% The ref columns of one type, as Column-ChildType pairs.
type_ref_columns(Types, Name, RefColumns) :-
    type_definition(Types, Name, Columns, ColumnTypes),
    findall(Column-ChildType,
            ( nth1(Position, Columns, Column),
              nth1(Position, ColumnTypes, ChildType),
              declared_type_name(Types, ChildType) ),
            RefColumns).

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
% keys (verdict: interned_graph_is_a_dag) -- so this is a refusal, not a
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
% whose value does not match the declared struct shape is a NAMED refusal at
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
% keys_not_sorted refusal guarded -- oracle term identity treating
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
field_shape_error(_, TypeName, Column, text, ChildValue, Reason) :-
    \+ atomic(ChildValue), !,
    Reason = field_not_text(TypeName, Column, ChildValue).

% Both surface spellings of a JSON object reach here: the raw braces literal
% a fixture or a host row writes, and the canonical obj(SortedPairs) body.pl
% produces. Sorted pairs, always, so a caller may index by column name.
json_object_value(Value, Pairs) :-
    nonvar(Value),
    (   Value = obj(RawPairs)
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
canonicalize_world_rows(Decls, Rows0, Rows) :-
    type_definitions(Decls, Types),
    (   Types == []
    ->  Rows = Rows0
    ;   maplist(canonicalize_signed_row(Decls, Types), Rows0, Rows)
    ).

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
    ;   Value = Value0
    ).

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
% This is a decl-driven refusal, not an execution change: a row that passes
% behaves exactly as it did before the type existed. It runs where the data
% is static (fixture Initial + Schedule on both doors); the emitted runtime
% repeats it at intern time, where the data is not.
world_row_shape_violation(Decls, Rows, mismatch(Ref, Column, TypeName, Reason)) :-
    type_definitions(Decls, Types),
    Types \== [],
    member(SignedRow, Rows),
    bare_row(SignedRow, Row),
    compound(Row),
    functor(Row, Name, Arity),
    Ref = Name/Arity,
    ref_column_names(Decls, Ref, Arity, Columns),
    nth1(Position, Columns, Column),
    memberchk(col_type(Ref, Column, TypeName), Decls),
    declared_type_name(Types, TypeName),
    arg(Position, Row, Value),
    nonvar(Value),
    type_shape_error(Types, TypeName, Value, Reason),
    !.

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

escape_json_codes([], []).
escape_json_codes([Code | Rest], Out) :-
    (   Code =:= 0'"  -> Escaped = [0'\\, 0'"]
    ;   Code =:= 0'\\ -> Escaped = [0'\\, 0'\\]
    ;   Code =:= 10   -> Escaped = [0'\\, 0'n]
    ;   Code =:= 9    -> Escaped = [0'\\, 0't]
    ;   Code < 32     -> format(atom(HexAtom), '\\u~`0t~16r~4|', [Code]), atom_codes(HexAtom, Escaped)
    ;   Escaped = [Code]
    ),
    escape_json_codes(Rest, RestOut),
    append(Escaped, RestOut, Out).
