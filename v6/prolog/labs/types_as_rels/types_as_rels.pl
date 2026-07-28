% types_as_rels.pl : LAB ENTRY.
%
% Run: swipl -q -l v6/prolog/labs/types_as_rels/types_as_rels.pl -g go -g halt
%
% Grades THE UNIFICATION HYPOTHESIS (plans/2026-07-28-types-as-rels-header.md):
% struct / enum / type are shorthands over rel, with a policy bundle pinned
% (content-addressed surrogate identity, no mutation, refcount lifetime), and
% nesting is never physical -- a nested position is a ref column, the tree
% lives in the printer and the matcher.
%
% Six check groups: json round-trip, policy-bundle derivation, the domination
% scenario pair (support counting vs SQL ON DELETE CASCADE on the SAME store),
% match-path lowering at depth 1/2/3, compactness pricing of three decl
% spellings, and the merge bit (kind words as named lattices).
%
% Verdict and the priced Q1-Q8 tables: plans/2026-07-28-types-as-rels-verdict.md.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('../../src/grader.pl').

:- discontiguous check/2.
:- use_module(schema).
:- use_module(value_model).
:- use_module(lowering).
:- use_module(lattice).

go :- run(check).

% ═══ scenario builders ══════════════════════════════════════════════════════

tree_store(MintPolicy, ListMode, Name, Store, Id) :-
    empty_store(MintPolicy, ListMode, Store0),
    example_value(Name, Json),
    lower_value(struct(route), Json, Store0, Store, id(Id)).

% The domination pair: two roots sharing one view value.
shared_store(MintPolicy, Store, IdA, IdB) :-
    empty_store(MintPolicy, indexed, Store0),
    example_value(tree_a, JsonA),
    lower_value(struct(route), JsonA, Store0, Store1, id(IdA)),
    add_root(IdA, Store1, Store2),
    example_value(tree_b, JsonB),
    lower_value(struct(route), JsonB, Store2, Store3, id(IdB)),
    add_root(IdB, Store3, Store).

dangling_refs(Store, Dangling) :-
    store_rows(Store, Rows),
    store_edges(Store, Edges),
    findall(ref(ParentId, ChildId),
            ( ( member(row(_, ParentId, _, Args, _), Rows), member(id(ChildId), Args)
              ; member(edge(elem, ParentId, _, id(ChildId)), Edges) ),
              \+ memberchk(row(_, ChildId, _, _, _), Rows) ),
            Dangling).

% The insert / share / release op tape for the policy-bundle check. Deltas are
% derived the way the engine derives them: diff the row set across the op
% (rulings.pl r7_boundary_diff, set diff on Set rels).
apply_op(Op, Store0, Store, Deltas) :-
    store_rows(Store0, Before),
    run_op(Op, Store0, Store),
    store_rows(Store, After),
    findall(-row(Type, Id),
            ( member(row(Type, Id, _, _, _), Before),
              \+ memberchk(row(_, Id, _, _, _), After) ), Removed),
    findall(+row(Type, Id),
            ( member(row(Type, Id, _, _, _), After),
              \+ memberchk(row(_, Id, _, _, _), Before) ), Added),
    append(Removed, Added, Deltas).

run_op(insert(Spec, Name), Store0, Store) :-
    example_value(Name, Json),
    lower_value(Spec, Json, Store0, Store1, id(Id)),
    add_root(Id, Store1, Store).
run_op(release(Seq), Store0, Store) :-
    seq_id(Store0, Seq, Id),
    release_root(Id, Store0, Store, _).

run_tape([], Store, Store, []).
run_tape([Op | Rest], Store0, Store, [Deltas | More]) :-
    apply_op(Op, Store0, Store1, Deltas),
    run_tape(Rest, Store1, Store, More).

% ═══ CHECK 1: JSON ROUND-TRIP ═══════════════════════════════════════════════

check(round_trip_term_identical,
      ( example_value(route_tree, Json),
        tree_store(content, indexed, route_tree, Store, Id),
        render_value(id(Id), Store, Back),
        Back == Json )).

check(round_trip_text_byte_identical,
      ( example_value(route_tree, Json),
        json_text(Json, Text),
        tree_store(content, indexed, route_tree, Store, Id),
        render_value(id(Id), Store, Back),
        json_text(Back, BackText),
        BackText == Text,
        atom_length(Text, Length), Length =:= 245 )).

% The shared subtree: the tree TEXT names the view twice, the graph holds one
% row. This is the whole "nesting is never physical" claim in one check.
check(shared_subtree_stored_once,
      ( tree_store(content, indexed, route_tree, Store, Id),
        rows_of_type(Store, view, Views), length(Views, 1),
        rows_of_type(Store, body_page, Pages), length(Pages, 1),
        render_value(id(Id), Store, Back),
        json_text(Back, BackText),
        findall(Position,
                sub_atom(BackText, Position, _, _, '"title":"T"'), Positions),
        length(Positions, 2) )).

check(worked_example_row_and_edge_counts,
      ( tree_store(content, indexed, route_tree, Store, _),
        row_count(Store, Rows), Rows =:= 9,
        store_edges(Store, Edges), length(Edges, EdgeCount), EdgeCount =:= 4,
        rows_of_type(Store, route, Routes), length(Routes, 3),
        rows_of_type(Store, list, Lists), length(Lists, 3) )).

% Cons cells share every SUFFIX; an indexed header row shares only the whole
% list. Same two values, two storage shapes, measured.
check(cons_shares_tails_indexed_does_not,
      ( empty_store(content, cons, ConsStore0),
        example_value(tags_short, ShortJson),
        example_value(tags_long, LongJson),
        lower_value(list(text), ShortJson, ConsStore0, ConsStore1, _),
        lower_value(list(text), LongJson, ConsStore1, ConsStore, _),
        row_count(ConsStore, ConsRows), ConsRows =:= 4,
        empty_store(content, indexed, IndexStore0),
        lower_value(list(text), ShortJson, IndexStore0, IndexStore1, _),
        lower_value(list(text), LongJson, IndexStore1, IndexStore, _),
        row_count(IndexStore, IndexRows), IndexRows =:= 2,
        store_edges(IndexStore, IndexEdges), length(IndexEdges, 5) )).

check(cons_round_trips_too,
      ( example_value(tags_long, Json),
        empty_store(content, cons, Store0),
        lower_value(list(text), Json, Store0, Store, id(Id)),
        render_value(id(Id), Store, Back),
        Back == Json )).

% ═══ CHECK 2: POLICY-BUNDLE DERIVATION ══════════════════════════════════════
% insert / share / release, graded against the hypothesis table. Nothing here
% needs a construct the language does not have: a keyed Set rel whose key is
% its content columns, plus a bind that computes the id from the content.

check(policy_bundle_insert_share_release,
      ( empty_store(counter, indexed, Store0),
        Tape = [ insert(list(text), tags_short),
                 insert(list(text), tags_short),
                 release(1),
                 release(1) ],
        run_tape(Tape, Store0, Store, Deltas),
        Deltas == [ [+row(list, 1)], [], [], [-row(list, 1)] ],
        row_count(Store, 0) )).

% The share step is a no-op delta because an equal-row write is a no-op
% (rulings.pl r_equal_row_write). Support still rises: two supports, one row.
check(policy_bundle_support_counts,
      ( empty_store(counter, indexed, Store0),
        run_op(insert(list(text), tags_short), Store0, Store1),
        support(Store1, 1, One), One =:= 1,
        run_op(insert(list(text), tags_short), Store1, Store2),
        support(Store2, 1, Two), Two =:= 2,
        run_op(release(1), Store2, Store3),
        support(Store3, 1, Back), Back =:= 1 )).

% ═══ CHECK 3: DOMINATION SCENARIO PAIR ══════════════════════════════════════
% Mint order under the counter policy (hand-computable, quoted in the verdict):
%   1 list("x","y")   2 view   3 body_page   4 list()   5 route "/a"
%   6 route "/b"      (tree_b reuses 1..4 by content)

check(domination_mint_order,
      ( shared_store(counter, Store, IdA, IdB),
        IdA =:= 5, IdB =:= 6,
        row_count(Store, 6),
        support(Store, 3, Shared), Shared =:= 2 )).

check(domination_shared_child_survives,
      ( shared_store(counter, Store0, IdA, _),
        release_root(IdA, Store0, Store, Removed),
        Removed == [row(route, 5)],
        row_count(Store, 5),
        support(Store, 3, One), One =:= 1 )).

check(domination_sole_owner_cascades,
      ( shared_store(counter, Store0, IdA, IdB),
        release_root(IdA, Store0, Store1, _),
        release_root(IdB, Store1, Store, Removed),
        Removed == [ row(body_page, 3), row(list, 1), row(list, 4),
                     row(route, 6), row(view, 2) ],
        row_count(Store, 0) )).

check(cascade_leaves_no_orphan_edges,
      ( shared_store(counter, Store0, IdA, IdB),
        release_root(IdA, Store0, Store1, _),
        release_root(IdB, Store1, Store, _),
        orphan_edges(Store, []),
        store_edges(Store, []) )).

% SQL's own cascade is the WRONG semantics for a shared child: ON DELETE
% CASCADE walks the parent's children regardless of who else points at them.
check(fk_cascade_kills_shared_child,
      ( shared_store(counter, Store0, IdA, _),
        fk_release_root(IdA, Store0, Store, Removed),
        Removed == [ row(body_page, 3), row(list, 1), row(list, 4),
                     row(route, 5), row(view, 2) ],
        row_count(Store, 1) )).

check(fk_cascade_leaves_dangling_refs,
      ( shared_store(counter, Store0, IdA, _),
        fk_release_root(IdA, Store0, Store, _),
        dangling_refs(Store, Dangling),
        msort(Dangling, Sorted),
        Sorted == [ref(6, 3), ref(6, 4)] )).

% A value that ALSO arrived as its own row (an EDB origin, or any second
% demand) never cascades: its support never reaches zero. Per ARCH.pl:68-71
% support is per-ROW origin, so this is not a defect, it is the rule -- but it
% is the thing that makes "child dies with parent" untrue in general.
check(second_root_blocks_cascade,
      ( shared_store(counter, Store0, IdA, IdB),
        add_root(3, Store0, Store1),
        release_root(IdA, Store1, Store2, _),
        release_root(IdB, Store2, Store, Removed),
        Removed == [row(list, 4), row(route, 6)],
        row_count(Store, 3) )).

% ═══ CHECK: WHY REFCOUNTING IS COMPLETE HERE ════════════════════════════════

check(interned_graph_is_a_dag,
      ( shared_store(content, Store, _, _),
        refs_point_backward(Store) )).

check(support_equals_reachability,
      ( shared_store(content, Store, _, _),
        live_ids(Store, Live), reachable_ids(Store, Reachable),
        Live == Reachable )).

% The counterfactual: an EXTRINSIC-key graph (ids not computed from content)
% can cycle, and then support counting is no longer a complete collector.
% This is the scenario in which a struct cannot be an interned value.
check(extrinsic_key_cycle_leaks,
      ( CyclicStore = store(counter, indexed, 3,
            [ row(node, 1, [id(2)], [id(2)], 1),
              row(node, 2, [id(1)], [id(1)], 2) ], [], []),
        live_ids(CyclicStore, Live), Live == [1, 2],
        reachable_ids(CyclicStore, Reachable), Reachable == [],
        collect(CyclicStore, Collected, Removed),
        Removed == [],
        row_count(Collected, 2) )).

% ═══ CHECK: MINTING POLICY ══════════════════════════════════════════════════
% Content ids are a function of the value, so two build orders agree row for
% row. Counter ids do not, and the disagreement is exactly what a byte-diffed
% tick log would report as a failure.

check(content_ids_order_independent,
      ( shared_store(content, Forward, _, _),
        empty_store(content, indexed, Store0),
        example_value(tree_b, JsonB),
        lower_value(struct(route), JsonB, Store0, Store1, _),
        example_value(tree_a, JsonA),
        lower_value(struct(route), JsonA, Store1, Reverse, _),
        row_keys(Forward, ForwardKeys), row_keys(Reverse, ReverseKeys),
        ForwardKeys == ReverseKeys )).

check(counter_ids_order_dependent,
      ( shared_store(counter, Forward, _, _),
        empty_store(counter, indexed, Store0),
        example_value(tree_b, JsonB),
        lower_value(struct(route), JsonB, Store0, Store1, _),
        example_value(tree_a, JsonA),
        lower_value(struct(route), JsonA, Store1, Reverse, _),
        row_keys(Forward, ForwardKeys), row_keys(Reverse, ReverseKeys),
        ForwardKeys \== ReverseKeys )).

% and the repair: a tick log that prints the VALUE, not the id, is stable
% under both policies and both orders.
check(rendered_text_stable_under_both_policies,
      ( tree_store(content, indexed, route_tree, ContentStore, ContentId),
        render_value(id(ContentId), ContentStore, ContentJson),
        json_text(ContentJson, Text),
        tree_store(counter, indexed, route_tree, CounterStore, CounterId),
        render_value(id(CounterId), CounterStore, CounterJson),
        json_text(CounterJson, Text2),
        Text == Text2 )).

% The identity ASSIGNMENT, not just the id set: which value got which id.
% Comparing bare Type-Id pairs would be too weak to see the disagreement (two
% route rows swapping ids leaves the id set untouched).
row_keys(Store, Keys) :-
    store_rows(Store, Rows),
    findall(assign(Type, Key, Id), member(row(Type, Id, Key, _, _), Rows), Keys0),
    msort(Keys0, Keys).

% ═══ CHECK 4: MATCH-PATH LOWERING AT DEPTH 1 / 2 / 3 ════════════════════════

check(match_path_depth_1,
      ( match_path_sql(route, [path], Sql),
        Sql == 'SELECT r0."path" FROM "route" r0 WHERE r0."id" = ?' )).

check(match_path_depth_2,
      ( match_path_sql(route, [body-redirect, to], Sql),
        Sql == 'SELECT r1."to" FROM "route" r0 JOIN "body_redirect" r1 ON r1."id" = r0."body" WHERE r0."id" = ?' )).

check(match_path_depth_3,
      ( match_path_sql(route, [body-page, view, title], Sql),
        Sql == 'SELECT r2."title" FROM "route" r0 JOIN "body_page" r1 ON r1."id" = r0."body" JOIN "view" r2 ON r2."id" = r1."view" WHERE r0."id" = ?' )).

check(match_path_join_count_is_depth,
      ( forall(( member(Steps-Expected,
                        [ [path]-0,
                          [body-redirect, to]-1,
                          [body-page, view, title]-2 ]),
                 match_path_sql(route, Steps, Sql) ),
               ( findall(Position, sub_atom(Sql, Position, _, _, 'JOIN '), Positions),
                 length(Positions, Expected) )) )).

check(inline_json_alternative_is_one_table,
      ( inline_json_sql(route, '$.view.title', Sql),
        Sql == 'SELECT json_extract(r0."body", \'$.view.title\') FROM "route" r0 WHERE r0."id" = ?',
        \+ sub_atom(Sql, _, _, _, 'JOIN ') )).

% ═══ CHECK: DDL SHAPE ═══════════════════════════════════════════════════════

check(ddl_route_table,
      ( table_ddl(route, [Table, Unique]),
        Table == 'CREATE TABLE "route" ("id" INTEGER NOT NULL, "path" TEXT NOT NULL, "body" INTEGER NOT NULL, "children" INTEGER NOT NULL, PRIMARY KEY ("id")) WITHOUT ROWID',
        Unique == 'CREATE UNIQUE INDEX "route_content" ON "route" ("path", "body", "children")' )).

check(ddl_covers_every_declared_type,
      ( all_ddl(Ddl), length(Ddl, 8),
        findall(Type, struct_type(Type, _), Types), length(Types, 4) )).

% ═══ CHECK 5: COMPACTNESS PRICING ═══════════════════════════════════════════

check(spellings_expand_to_same_tables,
      ( spelling_tables(a, A), spelling_tables(b, B), spelling_tables(c, C),
        msort(A, Sorted), msort(B, Sorted), msort(C, Sorted),
        length(Sorted, 6) )).

check(spelling_char_counts,
      ( spelling_length(a, LengthA), LengthA =:= 165,
        spelling_length(b, LengthB), LengthB =:= 165,
        spelling_length(c, LengthC), LengthC =:= 232 )).

check(spelling_new_construct_counts,
      ( spelling_constructs(a, ConstructsA), length(ConstructsA, 5),
        spelling_constructs(b, ConstructsB), length(ConstructsB, 4),
        spelling_constructs(c, ConstructsC), length(ConstructsC, 0) )).

spelling_length(Name, Length) :-
    spelling_text(Name, Text), atom_length(Text, Length).

% ═══ CHECK 6: THE MERGE BIT (kind words as named lattices) ══════════════════
% Our own claim, graded: set / log / keyed are three named join-semilattices,
% not three mechanisms. Prior-art citations live in the verdict.

sample(set, [ [a, b], [b, c], [c] ]).
sample(log, [ [st(1,1)-a], [st(1,2)-a], [st(2,1)-b] ]).
sample(keyed, [ [k1-st(1,1)-v1], [k1-st(2,1)-v2], [k2-st(1,3)-v9] ]).

check(kind_words_are_joins,
      ( forall(( merge_kind(Kind, _), sample(Kind, [A, B, C]) ),
               ( lub(Kind, A, A, A),                       % idempotent
                 lub(Kind, A, B, AB), lub(Kind, B, A, BA), AB == BA,
                 lub(Kind, AB, C, Left),                   % associative
                 lub(Kind, B, C, BC), lub(Kind, A, BC, Right), Left == Right )) )).

check(arrivals_only_move_state_up,
      ( forall(( merge_kind(Kind, _), sample(Kind, Arrivals) ),
               ( merge_all(Kind, Arrivals, Final),
                 foldl(monotone_step(Kind), Arrivals, [], Final) )) )).

% log is idempotent ONLY because the stamp is part of the value. Strip the
% stamp and a second identical arrival is lost, which is exactly the
% occurrence semantics q1 preserves.
check(log_needs_its_stamp_to_be_a_join,
      ( sample(log, Arrivals),
        merge_all(log, Arrivals, Stamped), length(Stamped, 3),
        observed(log, Stamped, Rows), msort(Rows, Sorted), Sorted == [a, a, b],
        findall(Row, ( member(Arrival, Arrivals), member(_-Row, Arrival) ), Raw),
        sort(Raw, Deduped), length(Deduped, 2) )).

% THE POINT against R7: a keyed rel's lattice state only ever rises, and the
% boundary delta STILL retracts, because the observed projection changed.
% Monotone merge buys the store (no re-derivation), never the boundary.
check(keyed_state_rises_but_boundary_retracts,
      ( First = [k1-st(1,1)-v1], Second = [k1-st(2,1)-v2],
        lub(keyed, First, Second, State), leq(keyed, First, State),
        boundary(keyed, First, State, Deltas),
        Deltas == [-row(k1, v1), +row(k1, v2)] )).

check(set_boundary_never_retracts_on_arrival,
      ( First = [a, b], Second = [c],
        lub(set, First, Second, State),
        boundary(set, First, State, Deltas),
        Deltas == [+c] )).

monotone_step(Kind, Arrival, State0, State) :-
    lub(Kind, State0, Arrival, State),
    leq(Kind, State0, State).
