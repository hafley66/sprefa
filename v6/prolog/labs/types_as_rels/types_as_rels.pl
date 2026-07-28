% types_as_rels.pl : LAB ENTRY.
%
% Run: swipl -q -l v6/prolog/labs/types_as_rels/types_as_rels.pl -g go -g halt
%
% Grades THE UNIFICATION HYPOTHESIS (plans/2026-07-28-types-as-rels-header.md):
% struct / enum / type are shorthands over rel with an explicit value or
% entity policy. Value pins content identity, immutability and refcount
% lifetime. Entity pins extrinsic identity, mutable history and explicit
% lifetime. Nesting is never physical: a nested position is a ref column.
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
:- use_module('1_entity_model').
:- use_module('2_mate_model').
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

% ═══ FIXPOINT ROUND 1: ENTITY BREAK CASES ══════════════════════════════════
% Attack: apply every value-plane conclusion to an entity with an extrinsic
% id, mutable current row, immutable history, and explicit lifetime.

check(both_policies_require_explicit_decl,
      ( resolve_policy(decl(route, value), _),
        resolve_policy(decl(route, entity), _),
        \+ resolve_policy(decl(route), _) )).

check(both_policy_bundles_have_four_bits,
      ( policy_bundle(value, policy(ValueIdentity, ValueMutation,
                                    ValueLifetime, ValueMerge)),
        policy_bundle(entity, policy(EntityIdentity, EntityMutation,
                                     EntityLifetime, EntityMerge)),
        ValueIdentity == content_hash,
        ValueMutation == immutable,
        ValueLifetime == support_zero,
        ValueMerge == set,
        EntityIdentity == extrinsic_id,
        EntityMutation == mutable_history,
        EntityLifetime == explicit_retire,
        EntityMerge == keyed )).

check(entity_equal_content_mints_distinct_ids,
      ( empty_entity_store(Store0),
        create_entity(route, [text('/a')], Store0, Store1, FirstId, _),
        create_entity(route, [text('/a')], Store1, _, SecondId, _),
        FirstId =:= 1,
        SecondId =:= 2 )).

check(entity_update_preserves_id,
      ( empty_entity_store(Store0),
        create_entity(route, [text('/a')], Store0, Store1, RouteId, _),
        update_entity(RouteId, [text('/b')], Store1, Store, _),
        entity_rows(Store, Rows),
        Rows == [entity(route, RouteId, [text('/b')])] )).

check(entity_update_appends_history,
      ( empty_entity_store(Store0),
        create_entity(route, [text('/a')], Store0, Store1, RouteId, _),
        update_entity(RouteId, [text('/b')], Store1, Store, _),
        entity_history(Store, History),
        History == [ version(1, route, RouteId, [text('/a')]),
                     version(2, route, RouteId, [text('/b')]) ] )).

check(entity_update_boundary_retracts_and_adds,
      ( empty_entity_store(Store0),
        create_entity(route, [text('/a')], Store0, Store1, RouteId, _),
        update_entity(RouteId, [text('/b')], Store1, _, Deltas),
        Deltas == [ -row(route, RouteId, [text('/a')]),
                    +row(route, RouteId, [text('/b')]) ] )).

check(entity_cycle_can_be_constructed,
      ( empty_entity_store(Store0),
        create_entity(node, [], Store0, Store1, FirstId, _),
        create_entity(node, [id(FirstId)], Store1, Store2, SecondId, _),
        update_entity(FirstId, [id(SecondId)], Store2, Store, _),
        entity_refs(Store, FirstId, [SecondId]),
        entity_refs(Store, SecondId, [FirstId]) )).

check(entity_cycle_partial_retire_is_refused,
      ( empty_entity_store(Store0),
        create_entity(node, [], Store0, Store1, FirstId, _),
        create_entity(node, [id(FirstId)], Store1, Store2, SecondId, _),
        update_entity(FirstId, [id(SecondId)], Store2, Store, _),
        \+ retire_entities([FirstId], Store, _, _) )).

check(entity_cycle_explicit_set_retires,
      ( empty_entity_store(Store0),
        create_entity(node, [], Store0, Store1, FirstId, _),
        create_entity(node, [id(FirstId)], Store1, Store2, SecondId, _),
        update_entity(FirstId, [id(SecondId)], Store2, Store3, _),
        retire_entities([FirstId, SecondId], Store3, Store, Deltas),
        Deltas == [ -row(node, FirstId, [id(SecondId)]),
                    -row(node, SecondId, [id(FirstId)]) ],
        entity_row_count(Store, 0),
        entity_history(Store, History),
        length(History, 3) )).

check(entity_lifetime_does_not_follow_refcount,
      ( empty_entity_store(Store0),
        create_entity(route, [text('/a')], Store0, Store, _, _),
        entity_row_count(Store, 1) )).

% ═══ FIXPOINT ROUND 2: SURROGATE MATE AND COEXISTENCE ══════════════════════
% Attack: keep semantic identity content-addressed while replacing the stored
% ref with a dense integer. Then require all three policy-placement options to
% assign the worked example identically and without an omitted choice.

mate_pair([FirstContent, SecondContent], Store, Deltas) :-
    empty_mate_store(Store0),
    intern_value(view, FirstContent, Store0, Store1, _, _, FirstDeltas),
    intern_value(view, SecondContent, Store1, Store, _, _, SecondDeltas),
    append(FirstDeltas, SecondDeltas, Deltas).

check(surrogate_semantic_ids_order_independent,
      ( semantic_id(view, [text('T')], TitleHash),
        mate_pair([[text('T')], [text('U')]], Forward, _),
        mate_pair([[text('U')], [text('T')]], Reverse, _),
        dense_for(Forward, TitleHash, _),
        dense_for(Reverse, TitleHash, _) )).

check(surrogate_dense_keys_order_dependent,
      ( mate_pair([[text('T')], [text('U')]], Forward, _),
        mate_pair([[text('U')], [text('T')]], Reverse, _),
        mate_assignments(Forward, ForwardAssignments),
        mate_assignments(Reverse, ReverseAssignments),
        ForwardAssignments \== ReverseAssignments )).

check(surrogate_tick_add_prints_value,
      ( empty_mate_store(Store0),
        intern_value(view, [text('T')], Store0, _, DenseId, Hash, Deltas),
        integer(DenseId),
        atom(Hash),
        Deltas == [+value(view, [text('T')])] )).

check(surrogate_share_release_refcounts,
      ( empty_mate_store(Store0),
        intern_value(view, [text('T')], Store0, Store1, _, Hash, FirstDeltas),
        intern_value(view, [text('T')], Store1, Store2, _, Hash, ShareDeltas),
        mate_support(Store2, Hash, 2),
        release_value(view, [text('T')], Store2, Store3, Hash, FirstRelease),
        mate_support(Store3, Hash, 1),
        release_value(view, [text('T')], Store3, _, Hash, LastRelease),
        FirstDeltas == [+value(view, [text('T')])],
        ShareDeltas == [],
        FirstRelease == [],
        LastRelease == [-value(view, [text('T')])] )).

check(surrogate_reintern_changes_dense_not_semantic,
      ( empty_mate_store(Store0),
        intern_value(view, [text('T')], Store0, Store1, FirstDense, Hash, _),
        release_value(view, [text('T')], Store1, Store2, Hash, _),
        intern_value(view, [text('U')], Store2, Store3, _, _, _),
        intern_value(view, [text('T')], Store3, _, SecondDense, Hash, _),
        FirstDense =\= SecondDense )).

check(parent_semantic_hash_ignores_dense_mate,
      ( semantic_id(view, [text('T')], ChildHash),
        mate_pair([[text('T')], [text('U')]], Forward, _),
        mate_pair([[text('U')], [text('T')]], Reverse, _),
        dense_for(Forward, ChildHash, ForwardDense),
        dense_for(Reverse, ChildHash, ReverseDense),
        ForwardDense =\= ReverseDense,
        semantic_id(body_page, [ref(ChildHash)], ForwardParentHash),
        semantic_id(body_page, [ref(ChildHash)], ReverseParentHash),
        ForwardParentHash == ReverseParentHash )).

check(parent_hash_from_dense_would_be_order_dependent,
      ( semantic_id(view, [text('T')], ChildHash),
        mate_pair([[text('T')], [text('U')]], Forward, _),
        mate_pair([[text('U')], [text('T')]], Reverse, _),
        dense_for(Forward, ChildHash, ForwardDense),
        dense_for(Reverse, ChildHash, ReverseDense),
        semantic_id(body_page, [storage(ForwardDense)], ForwardParentHash),
        semantic_id(body_page, [storage(ReverseDense)], ReverseParentHash),
        ForwardParentHash \== ReverseParentHash )).

check(coexistence_spellings_assign_same_policies,
      ( coexistence_assignments(decl_word, DeclAssignments),
        coexistence_assignments(use_site, UseAssignments),
        coexistence_assignments(hybrid, HybridAssignments),
        DeclAssignments == UseAssignments,
        UseAssignments == HybridAssignments,
        forall(coexistence_spelling(Option, _),
               coexistence_text(Option, _)) )).

check(coexistence_policy_token_counts,
      ( coexistence_policy_tokens(decl_word, 4),
        coexistence_policy_tokens(use_site, 7),
        coexistence_policy_tokens(hybrid, 7) )).

check(policy_specific_ddl_shapes,
      ( policy_ddl(value, view, ValueDdl),
        ValueDdl = [ValueCurrent, _],
        sub_atom(ValueCurrent, _, _, _, '"semantic" TEXT NOT NULL UNIQUE'),
        policy_ddl(entity, route, EntityDdl),
        EntityDdl = [EntityCurrent, EntityHistory],
        sub_atom(EntityCurrent, _, _, _, '"route_entity"'),
        sub_atom(EntityHistory, _, _, _, '"route_entity_history"'),
        sub_atom(EntityHistory, _, _, _, 'PRIMARY KEY ("id", "tick")') )).

% ═══ FIXPOINT ROUND 3: CROSS-POLICY BREAK CASES ════════════════════════════
% Attack: compose the two policies across refs, variable-arity rows, enum
% changes, matching, support release, rendering, and retirement.

check(entity_variable_arity_update_preserves_id,
      ( empty_entity_store(Store0),
        create_entity(list, [text(x)], Store0, Store1, ListId, _),
        update_entity(ListId, [text(w), text(x), text(y)], Store1, Store, _),
        entity_rows(Store, [entity(list, ListId,
                                   [text(w), text(x), text(y)])]) )).

check(entity_enum_variant_change_preserves_id_and_history,
      ( empty_entity_store(Store0),
        create_entity(body, [tag(page), id(7)], Store0, Store1, BodyId, _),
        update_entity(BodyId, [tag(redirect), text('/a')], Store1, Store, _),
        entity_rows(Store,
                    [entity(body, BodyId, [tag(redirect), text('/a')])]),
        entity_history(Store,
          [ version(1, body, BodyId, [tag(page), id(7)]),
            version(2, body, BodyId, [tag(redirect), text('/a')]) ]) )).

check(value_to_entity_deep_render_can_change,
      ( empty_entity_store(Store0),
        create_entity(view, [text('T')], Store0, Store1, ViewId, _),
        semantic_id(wrapper, [entity_ref(view, ViewId)], WrapperHash),
        entity_rows(Store1, [entity(view, ViewId, BeforeArgs)]),
        update_entity(ViewId, [text('U')], Store1, Store2, _),
        entity_rows(Store2, [entity(view, ViewId, AfterArgs)]),
        semantic_id(wrapper, [entity_ref(view, ViewId)], WrapperHash),
        BeforeArgs \== AfterArgs )).

check(value_to_entity_opaque_text_stays_stable,
      ( empty_entity_store(Store0),
        create_entity(view, [text('T')], Store0, Store1, ViewId, _),
        json_text(obj([view-obj([entity-int(ViewId)])]), BeforeText),
        update_entity(ViewId, [text('U')], Store1, _, _),
        json_text(obj([view-obj([entity-int(ViewId)])]), AfterText),
        BeforeText == AfterText )).

check(cross_policy_ref_modes_are_explicit,
      ( ref_mode(value, value, deep),
        ref_mode(entity, value, deep),
        ref_mode(entity, entity, deep),
        ref_mode(value, entity, identity),
        \+ ref_mode(value, entity, deep),
        \+ ref_mode(value, entity, default) )).

check(entity_ref_replacement_releases_old_value,
      ( empty_mate_store(MateStore0),
        intern_value(view, [text('T')], MateStore0, MateStore1,
                     FirstDense, FirstHash, _),
        empty_entity_store(EntityStore0),
        create_entity(route, [value_ref(FirstDense)], EntityStore0,
                      EntityStore1, RouteId, _),
        intern_value(view, [text('U')], MateStore1, MateStore2,
                     SecondDense, SecondHash, Added),
        update_entity(RouteId, [value_ref(SecondDense)], EntityStore1,
                      _, EntityDeltas),
        release_value(view, [text('T')], MateStore2, MateStore3,
                      FirstHash, Removed),
        mate_support(MateStore3, SecondHash, 1),
        Added == [+value(view, [text('U')])],
        Removed == [-value(view, [text('T')])],
        EntityDeltas == [ -row(route, RouteId, [value_ref(FirstDense)]),
                          +row(route, RouteId, [value_ref(SecondDense)]) ] )).

check(lifetime_claim_is_scoped_by_policy,
      ( shared_store(content, ValueStore, _, _),
        live_ids(ValueStore, ValueLive),
        reachable_ids(ValueStore, ValueReachable),
        ValueLive == ValueReachable,
        empty_entity_store(EntityStore0),
        create_entity(route, [text('/a')], EntityStore0, EntityStore, _, _),
        entity_row_count(EntityStore, 1) )).

check(match_path_cost_is_policy_independent,
      ( policy_match_path_sql(value, route,
                              [body-page, view, title], ValueSql),
        policy_match_path_sql(entity, route,
                              [body-page, view, title], EntitySql),
        ValueSql == EntitySql,
        findall(Position,
                sub_atom(ValueSql, Position, _, _, 'JOIN '), Joins),
        length(Joins, 2) )).

check(entity_retire_tick_prints_current_row,
      ( empty_entity_store(Store0),
        create_entity(route, [text('/a')], Store0, Store1, RouteId, _),
        update_entity(RouteId, [text('/b')], Store1, Store2, _),
        retire_entities([RouteId], Store2, Store, Deltas),
        Deltas == [-row(route, RouteId, [text('/b')])],
        entity_history(Store, History),
        length(History, 2) )).

round_findings(1,
    [ explicit_policy, four_bit_bundles, distinct_entity_ids,
      mutable_current, immutable_history, entity_boundary_replace,
      entity_cycles, atomic_retire, explicit_lifetime, scoped_support_gc ]).
round_findings(2,
    [ semantic_hash, dense_mate, value_tick_log, mate_refcount,
      reinterned_dense_key, semantic_child_hash, dense_child_hazard,
      coexistence_equivalence, policy_token_cost, policy_ddl ]).
round_findings(3,
    [ entity_variable_arity, mutable_enum_variant, deep_render_hazard,
      opaque_entity_ref, explicit_ref_direction, entity_value_support,
      scoped_lifetime, policy_independent_join, current_retire_delta ]).
round_findings(4, []).

check(fixpoint_stops_after_full_zero_finding_round,
      ( round_findings(1, First), First \== [],
        round_findings(2, Second), Second \== [],
        round_findings(3, Third), Third \== [],
        round_findings(4, []) )).
