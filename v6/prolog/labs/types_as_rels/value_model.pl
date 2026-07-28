% value_model.pl : the reference semantics for "a nested value IS rel rows".
%
% One store, four moving parts:
%
%   1. INTERN. A value row's id is a function of (type, key columns). Two
%      minting policies are modelled side by side so the lab can grade the
%      difference: content (sha of the type+key term, the value policy per
%      rulings.pl salt_minting) and counter (a monotone integer, the
%      storage_integer_keys ruling's dense-id shape).
%   2. NESTING. Every nested position is a ref column holding a child id.
%      Never inline. Lists get two modellings, indexed (a header row plus an
%      elem EDGE table) and cons (fixed-arity cells), because they price
%      differently on sharing.
%   3. SUPPORT. A row is live while something references it: another live
%      row's ref column, a live edge, or a root (an outside demand row).
%      Support hitting zero is the ONLY deletion mechanism.
%   4. THE FK COUNTERFACTUAL. fk_release_root/4 implements SQL's ON DELETE
%      CASCADE reading of the same store so the two can be graded on the same
%      scenario.
%
% Store term:
%   store(MintPolicy, ListMode, NextCounter, Rows, Edges, Roots)
%   Rows  : row(Type, Id, Key, Args, Seq)     Seq = mint order, 1-based
%   Edges : edge(elem, ParentId, Index, Ref)
%   Roots : list of Id (outside demand; a root is one unit of support)
%   Ref   : text(Atom) | int(Number) | id(Id)

:- module(value_model,
          [ empty_store/3, store_rows/2, store_edges/2, store_roots/2,
            rows_of_type/3, row_count/2, seq_id/3, id_seq/3,
            lower_value/5, render_value/3, json_text/2,
            support/3, add_root/3, drop_root/3,
            collect/3, release_root/4, fk_release_root/4,
            reachable_ids/2, live_ids/2, refs_point_backward/1,
            orphan_edges/2, intern_row/7 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(schema).

% ═══ store basics ═══════════════════════════════════════════════════════════

empty_store(MintPolicy, ListMode, store(MintPolicy, ListMode, 1, [], [], [])).

store_rows(store(_, _, _, Rows, _, _), Rows).
store_edges(store(_, _, _, _, Edges, _), Edges).
store_roots(store(_, _, _, _, _, Roots), Roots).

rows_of_type(Store, Type, Selected) :-
    store_rows(Store, Rows),
    include(row_has_type(Type), Rows, Selected).

row_has_type(Type, row(Type, _, _, _, _)).

row_count(Store, Count) :- store_rows(Store, Rows), length(Rows, Count).

% seq_id/3 reads the id minted at position Seq; the domination checks quote
% ids by mint order so the expected tick logs stay hand-readable under BOTH
% minting policies.
seq_id(Store, Seq, Id) :-
    store_rows(Store, Rows), memberchk(row(_, Id, _, _, Seq), Rows).

id_seq(Store, Id, Seq) :-
    store_rows(Store, Rows), memberchk(row(_, Id, _, _, Seq), Rows).

% ═══ interning ══════════════════════════════════════════════════════════════
% intern_row(Type, Key, Args, ChildEdges, Store0, Store, Id).
% ChildEdges = list of Ref, materialized as edge(elem, Id, Index, Ref) rows.
% Key is what identity is computed from; Args is what the row stores.

intern_row(Type, Key, _, _, Store0, Store0, Id) :-
    store_rows(Store0, Rows),
    memberchk(row(Type, Id, Key, _, _), Rows), !.
intern_row(Type, Key, Args, ChildRefs, Store0, Store, Id) :-
    Store0 = store(MintPolicy, ListMode, Counter, Rows, Edges, Roots),
    mint_id(MintPolicy, Type, Key, Counter, Id),
    length(Rows, Filled), Seq is Filled + 1,
    NextCounter is Counter + 1,
    numbered_edges(Id, 1, ChildRefs, NewEdges),
    append(Edges, NewEdges, Edges1),
    append(Rows, [row(Type, Id, Key, Args, Seq)], Rows1),
    Store = store(MintPolicy, ListMode, NextCounter, Rows1, Edges1, Roots).

% content: the id IS a function of the content, so it needs no counter, no
% uniqueness lock and no insertion order. counter: a dense monotone integer,
% cheap and small but order-dependent (graded in the entry file).
mint_id(content, Type, Key, _, Id) :-
    variant_sha1(value(Type, Key), Sha),
    sub_atom(Sha, 0, 6, _, Short),
    atom_concat('#', Short, Id).
mint_id(counter, _, _, Counter, Counter).

numbered_edges(_, _, [], []).
numbered_edges(ParentId, Index, [Ref | Rest], [edge(elem, ParentId, Index, Ref) | More]) :-
    Next is Index + 1,
    numbered_edges(ParentId, Next, Rest, More).

% ═══ lowering: json value -> rows ═══════════════════════════════════════════
% lower_value(Spec, Json, Store0, Store, Ref)

lower_value(text, str(Atom), Store, Store, text(Atom)) :- !.
lower_value(int, int(Number), Store, Store, int(Number)) :- !.

lower_value(struct(Type), obj(Pairs), Store0, Store, id(Id)) :- !,
    struct_type(Type, Fields),
    lower_fields(Type, Fields, Pairs, Store0, Store1, Refs),
    intern_row(Type, Refs, Refs, [], Store1, Store, Id).

lower_value(enum(Enum), obj(Pairs), Store0, Store, id(Id)) :- !,
    enum_type(Enum, Discriminator),
    memberchk(Discriminator-str(Tag), Pairs),
    variant_of(Enum, Tag, Table, Fields),
    lower_fields(Table, Fields, Pairs, Store0, Store1, Refs),
    intern_row(Table, Refs, Refs, [], Store1, Store, Id).

lower_value(list(ElementSpec), arr(Items), Store0, Store, id(Id)) :- !,
    Store0 = store(_, ListMode, _, _, _, _),
    lower_items(ElementSpec, Items, Store0, Store1, Refs),
    lower_list(ListMode, Refs, Store1, Store, Id).

lower_fields(_, [], _, Store, Store, []).
lower_fields(Type, [Field | Rest], Pairs, Store0, Store, [Ref | More]) :-
    field_spec(Type, Field, Spec),
    memberchk(Field-Json, Pairs),
    lower_value(Spec, Json, Store0, Store1, Ref),
    lower_fields(Type, Rest, Pairs, Store1, Store, More).

lower_items(_, [], Store, Store, []).
lower_items(Spec, [Json | Rest], Store0, Store, [Ref | More]) :-
    lower_value(Spec, Json, Store0, Store1, Ref),
    lower_items(Spec, Rest, Store1, Store, More).

% indexed: ONE header row per distinct whole list plus an elem edge per
% position. Sharing granularity = the whole list.
lower_list(indexed, Refs, Store0, Store, Id) :-
    intern_row(list, Refs, [], Refs, Store0, Store, Id).
% cons: fixed-arity cells. Sharing granularity = every SUFFIX (tail sharing).
lower_list(cons, Refs, Store0, Store, Id) :-
    intern_row(nil, [], [], [], Store0, Store1, NilId),
    foldl_cons(Refs, NilId, Store1, Store, Id).

foldl_cons([], TailId, Store, Store, TailId).
foldl_cons([Ref | Rest], NilId, Store0, Store, Id) :-
    foldl_cons(Rest, NilId, Store0, Store1, TailId),
    intern_row(cons, [Ref, id(TailId)], [Ref, id(TailId)], [], Store1, Store, Id).

% ═══ rendering: rows -> json value ══════════════════════════════════════════

render_value(text(Atom), _, str(Atom)).
render_value(int(Number), _, int(Number)).
render_value(id(Id), Store, Json) :-
    store_rows(Store, Rows),
    memberchk(row(Type, Id, _, Args, _), Rows),
    render_row(Type, Id, Args, Store, Json).

render_row(list, Id, _, Store, arr(Items)) :- !,
    store_edges(Store, Edges),
    findall(Index-Ref, member(edge(elem, Id, Index, Ref), Edges), Pairs0),
    msort(Pairs0, Pairs),
    findall(Item, ( member(_-Ref, Pairs), render_value(Ref, Store, Item) ), Items).
render_row(nil, _, _, _, arr([])) :- !.
render_row(cons, _, [Head, Tail], Store, arr([Item | Rest])) :- !,
    render_value(Head, Store, Item),
    render_value(Tail, Store, arr(Rest)).
render_row(Type, _, Args, Store, obj(Pairs)) :-
    variant_of(Enum, Tag, Type, Fields), !,
    enum_type(Enum, Discriminator),
    render_pairs(Fields, Args, Store, FieldPairs),
    Pairs = [Discriminator-str(Tag) | FieldPairs].
render_row(Type, _, Args, Store, obj(Pairs)) :-
    struct_type(Type, Fields),
    render_pairs(Fields, Args, Store, Pairs).

render_pairs([], [], _, []).
render_pairs([Field | Fields], [Ref | Refs], Store, [Field-Json | Rest]) :-
    render_value(Ref, Store, Json),
    render_pairs(Fields, Refs, Store, Rest).

% ═══ canonical json text (the byte-identical bar) ═══════════════════════════

json_text(str(Atom), Text) :- !, format(atom(Text), '"~w"', [Atom]).
json_text(int(Number), Text) :- !, format(atom(Text), '~w', [Number]).
json_text(entity-int(Number), Text) :- !, format(atom(Text), '~w', [Number]).
json_text(arr(Items), Text) :- !,
    maplist(json_text, Items, Texts),
    atomic_list_concat(Texts, ',', Inner),
    format(atom(Text), '[~w]', [Inner]).
json_text(obj(Pairs), Text) :-
    maplist(json_pair_text, Pairs, Texts),
    atomic_list_concat(Texts, ',', Inner),
    format(atom(Text), '{~w}', [Inner]).

json_pair_text(Key-Value, Text) :-
    json_text(Value, ValueText),
    format(atom(Text), '"~w":~w', [Key, ValueText]).

% ═══ support counting ═══════════════════════════════════════════════════════
% support(Store, Id, Count): live references to Id. Roots count as one each,
% matching "an outside demand row is a supporting row" (ARCH.pl:68-71, mixed
% heads are sound under count-IVM: support is per-ROW origin).

support(Store, Id, Count) :-
    store_rows(Store, Rows),
    store_edges(Store, Edges),
    store_roots(Store, Roots),
    findall(1, ( member(row(_, _, _, Args, _), Rows), member(id(Id), Args) ), FromRows),
    findall(1, member(edge(elem, _, _, id(Id)), Edges), FromEdges),
    findall(1, member(Id, Roots), FromRoots),
    append([FromRows, FromEdges, FromRoots], All),
    length(All, Count).

add_root(Id, store(P, L, C, Rows, Edges, Roots), store(P, L, C, Rows, Edges, [Id | Roots])).

drop_root(Id, store(P, L, C, Rows, Edges, Roots0), store(P, L, C, Rows, Edges, Roots)) :-
    selectchk(Id, Roots0, Roots).

% ═══ collection = one tick's fixpoint ═══════════════════════════════════════
% Set-at-a-time, exactly like the reference engine recomputes a level closure
% per tick (engine.pl:286, 295): every zero-support row goes at once, then the
% next round, until nothing is zero. The ORDER inside the tick is not
% observable; the boundary delta is a SET (rulings.pl r7_boundary_diff), so
% collect/3 returns the sorted removal set.

collect(Store0, Store, Removed) :-
    store_rows(Store0, Rows),
    findall(row(Type, Id),
            ( member(row(Type, Id, _, _, _), Rows), support(Store0, Id, 0) ),
            Zero),
    (   Zero == []
    ->  Store = Store0, Removed = []
    ;   foldl(delete_row, Zero, Store0, Store1),
        collect(Store1, Store, More),
        append(Zero, More, Removed0),
        msort(Removed0, Removed)
    ).

delete_row(row(_, Id), store(P, L, C, Rows0, Edges0, Roots), store(P, L, C, Rows, Edges, Roots)) :-
    exclude(row_with_id(Id), Rows0, Rows),
    exclude(edge_from(Id), Edges0, Edges).

row_with_id(Id, row(_, Id, _, _, _)).
edge_from(Id, edge(_, Id, _, _)).

% release_root/4: drop one outside demand row, then collect. One tick.
release_root(Id, Store0, Store, Removed) :-
    drop_root(Id, Store0, Store1),
    collect(Store1, Store, Removed).

% ═══ the FK counterfactual: SQL ON DELETE CASCADE ═══════════════════════════
% A FOREIGN KEY ... ON DELETE CASCADE deletes the referenced child because its
% PARENT died, with no regard for other referrers. Modelled exactly that way
% so the domination pair can be graded against it.

fk_release_root(Id, Store0, Store, Removed) :-
    drop_root(Id, Store0, Store1),
    store_rows(Store1, Rows),
    memberchk(row(Type, Id, _, _, _), Rows),
    fk_cascade([Id], Store1, Store, Removed0),
    msort([row(Type, Id) | Removed0], Removed).

fk_cascade([], Store, Store, []).
fk_cascade([Id | Rest], Store0, Store, Removed) :-
    store_rows(Store0, Rows),
    (   memberchk(row(_, Id, _, Args, _), Rows)
    ->  store_edges(Store0, Edges),
        findall(Child, member(id(Child), Args), FromArgs),
        findall(Child, member(edge(elem, Id, _, id(Child)), Edges), FromEdges),
        append(FromArgs, FromEdges, Children),
        findall(row(ChildType, Child),
                ( member(Child, Children), memberchk(row(ChildType, Child, _, _, _), Rows) ),
                ChildRows),
        delete_row(row(_, Id), Store0, Store1),
        append(Rest, Children, Queue),
        fk_cascade(Queue, Store1, Store, More),
        append(ChildRows, More, Removed)
    ;   fk_cascade(Rest, Store0, Store, Removed)
    ).

% ═══ reachability, the theorem side ═════════════════════════════════════════

reachable_ids(Store, Ids) :-
    store_roots(Store, Roots),
    reach_closure(Roots, Store, [], Ids0),
    sort(Ids0, Ids).

reach_closure([], _, Seen, Seen).
reach_closure([Id | Rest], Store, Seen0, Seen) :-
    (   memberchk(Id, Seen0)
    ->  reach_closure(Rest, Store, Seen0, Seen)
    ;   store_rows(Store, Rows),
        store_edges(Store, Edges),
        (   memberchk(row(_, Id, _, Args, _), Rows)
        ->  findall(Child, member(id(Child), Args), FromArgs),
            findall(Child, member(edge(elem, Id, _, id(Child)), Edges), FromEdges),
            append(FromArgs, FromEdges, Children)
        ;   Children = []
        ),
        append(Rest, Children, Queue),
        reach_closure(Queue, Store, [Id | Seen0], Seen)
    ).

live_ids(Store, Ids) :-
    store_rows(Store, Rows),
    findall(Id, ( member(row(_, Id, _, _, _), Rows), support(Store, Id, Count), Count > 0 ), Ids0),
    sort(Ids0, Ids).

% Every ref points at a STRICTLY EARLIER mint. This is why refcounting is a
% complete collector here: an interned value graph cannot contain a cycle,
% because a parent's identity is computed FROM its children's ids.
refs_point_backward(Store) :-
    store_rows(Store, Rows),
    store_edges(Store, Edges),
    forall(( member(row(_, _, _, Args, ParentSeq), Rows), member(id(ChildId), Args) ),
           ( id_seq(Store, ChildId, ChildSeq), ChildSeq < ParentSeq )),
    forall(( member(edge(elem, ParentId2, _, id(ChildId2)), Edges),
             id_seq(Store, ParentId2, ParentSeq2) ),
           ( id_seq(Store, ChildId2, ChildSeq2), ChildSeq2 < ParentSeq2 )).

orphan_edges(Store, Orphans) :-
    store_rows(Store, Rows),
    store_edges(Store, Edges),
    findall(Edge,
            ( member(Edge, Edges), Edge = edge(_, ParentId, _, _),
              \+ memberchk(row(_, ParentId, _, _, _), Rows) ),
            Orphans).
