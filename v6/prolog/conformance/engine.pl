% Reference interpreter for the conformance fixtures.
%
% Run: swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
%
% ── the tick, stated once ───────────────────────────────────────────────────
% 1. Outside arrivals reach the tick as an ORDERED LIST with duplicates
%    (occurrence lab: the honest input). +Row into a Log rel appends with an
%    engine stamp st(Tick, Seq); +Row into a Set rel is membership add; -Row
%    from a Log rel throws (occurrences cannot un-happen).
% 2. Trigger occurrences for edge rules = carry-in from the previous tick
%    (q4 next_tick), then outside arrivals, then level rows newly true after
%    arrivals. Each occurrence fires edge rules ONE AT A TIME in order.
% 3. Bare positive body atoms are trigger sources; latest/1 samples the
%    current visible relation without becoming a trigger source.
% 4. pre(Atom) reads the EVOLVING pre-state: the persistent store as writes
%    so far this tick left it, previous tick's level rows frozen. First
%    occurrence therefore reads T-1; later occurrences chain (r1 rider).
% 5. Writes: keyed head = replace (-old/+new at the boundary), equal row =
%    no-op; Log head = append with stamp; unkeyed Set head from an edge rule
%    throws edge_into_unkeyed_set. Two DIFFERENT rows for one key from ONE
%    occurrence throw keyed_conflict; across occurrences the later write is
%    the fold step (the static pairwise-disjointness law owns prevention).
% 6. Boundary deltas (r7): Log rels one +Row per new stamp (delta MULTISET);
%    Set rels and level views a set diff (removed then added). Intermediate
%    fold states are not observable (R2 rider).
% 7. Carry-out = edge-written rows + newly-true post-write level rows; they
%    are trigger occurrences for T+1. The engine self-schedules drain ticks
%    (empty outside-arrival set) while carry remains (q5 engine_owned),
%    capped so a self-feeding chain fails loudly.
% 8. now(Tick) is a kernel read of the current tick (R3), never an arrival.
% 9. Log rels require a keep/2 declaration (q10); keep(count(N)) prunes to
%    the newest N stamps at tick end; keep(all) is the explicit fixture
%    escape for unbounded history.
%
% ── aggregates (q7 bag, q9 reserved head forms, json arm) ───────────────────
% count/sum/min/max/json_array/json_object appear in level-rule head column
% positions only. Grouping is by the evaluated non-aggregate head columns;
% the bag of body derivations is the multiset aggregated over. json_array
% collects the bag in canonical (msort) order; json_object throws on one key
% with two values. Edge rules never carry aggregate heads.
%
% ── json values ─────────────────────────────────────────────────────────────
% Braces terms ARE the literal syntax ({ tag: v1 } reads as {}/1); the engine
% canonicalizes to obj(SortedPairs). decode(Expr, Pattern) is the body goal:
% object patterns are OPEN (extra keys ignored), missing key and none both
% fail a bare field pattern. json_each(List, Elem) is the element fan-out.
%
% ── out of scope here ───────────────────────────────────────────────────────
% Real effect transports (shell spawn, SSE): fixtures model bind fills as
% scheduled arrivals (the canned-rows law). Pattern/grammar matching
% (astgrep lab) is a separate reference matcher, not this file.

:- module(engine,
          [ run_fixture_checks/2, run_program/5, fixture_expectations_hold/2,
            rel_rows/3, rel_deltas/3,
            trigger_items/2, body_finalize_ref/2,
            body_latest_ref/2, body_pre_ref/2,
            check_program/1,
            rel_kind/3, decl_key/3 ]).
:- reexport(body, [json_canon/2]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).
:- use_module('../1_expansion', [expand_program/3]).
:- use_module('../next/1_expand/0_body_walk', [walk_body/3, body_wrapper_refs/4]).
% Shared with the compiler, the 1_host_expand.pl precedent: one module both
% doors call, so the cone cannot fork into two analyses.
:- use_module('../2_subscribe', [subscribed_rels/4]).
:- use_module('../0_program_check',
              [ first_violation/3, relation_kind/3, declared_key/3 ]).
:- use_module('../3_clock_check', [clock_violation/2]).
:- use_module('../0_type_plane',
              [ world_row_shape_violation/3,
                canonicalize_world_rows/3,
                normalize_relation_reference_rows/3
              ]).
:- use_module('../0_relation_pattern', [expand_relation_values/2]).
:- use_module('../0_option_expand', [acyclic_companion/5]).
:- use_module('../1_host_expand', [prepare_program/5, query_decl/3]).
:- use_module(rulings).
:- use_module(body).
:- use_module(level_eval).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- multifile user:fixture/5.
:- discontiguous user:fixture/5.

drain_cap(100).

% ═══ program shape ══════════════════════════════════════════════════════════
% prog(Decls, Rules)
%   Decls: kind(Ref, set|log) | keyed(Ref, Positions) | keep(Ref, Bound)
%   Rules: (Head <- Body) | (Head <+ Body)
% Bound: count(N) | all      (duration bounds arrive with the clock fixtures)


rel_kind(Decls, Ref, Kind) :- relation_kind(Decls, Ref, Kind).

decl_key(Decls, Ref, Positions) :- declared_key(Decls, Ref, Positions).

key_of(Positions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, Positions), nth1(Position, Args, Column) ), Key).

% Load-time program checks: headed relation compatibility, body markings,
% keyed-Log exclusion, and retention presence.
%
% The trigger conditions live in 0_program_check.pl, shared with the compiler.
% What stays here is this door's ORDER and this door's exception vocabulary,
% both of which are fixture data: the oracle throws bare terms, the compiler
% wraps in unsupported_construct/1, and a program violating two classes reports
% different ones at the two doors.
% STRUCT-AS-ROWS (ruling compound_storage = struct_as_rows): the declared
% value plane is checked first, ahead of every class that reads a column type,
% and the compiler's gate opens with the same two.
engine_check_order([ key_position_out_of_range,
                     key_position_duplicate,
                     type_cycle,
                     column_type_unknown,
                     % Ahead of the value-plane classes: a higher-order goal
                     % is not a relation atom at all, so nothing below has a
                     % meaningful question to ask about it.
                     dynamic_relation_name,
                     reserved_relation_value_carrier,
                     relation_pattern_not_a_relation_value,
                     % Straight after the concrete-argument class it extends:
                     % the same law where the offending value is a variable.
                     relation_column_type_conflict,
                     % Ruling type_gate_widening. Straight after the ref-column
                     % conflict class, whose statement it generalizes from ref
                     % types to every declared column type and narrows to the
                     % head direction; that class stays ahead of it so a
                     % program contradicting a STRUCT type still reports the
                     % ref-flavoured name it always did.
                     head_column_type_conflict,
                     cst_capture_unused,
                     cst_variable_uncaptured,
                     regexp_pattern_not_literal,
                     regexp_pattern_outside_subset,
                     regexp_pattern_invalid,
                     regexp_operand_not_text,
                     % Burrs B3/B4: shapes the compiler refused and this engine
                     % ran. Refused here too so the two doors answer the same
                     % program the same way (0_program_check.pl states why).
                     relation_value_under_negation,
                     relation_value_in_edge_rule,
                     % The slot analyze.pl gives the same class, so a program
                     % violating this and one of the classes below reports the
                     % same one at both doors. Before this entry every
                     % reserved word was read as an ordinary relation atom by
                     % solve/2's final clause and derived nothing, quietly.
                     reserved_body_word,
                     keyed_level_head,
                     keyed_log_rel,
                     log_on_level_headed_rel,
                     missing_retention,
                     keep_on_non_log_rel,
                     retention_head_conflict_risk,
                     aggregate_in_edge_head,
                     % After the edge class, which is the more specific fact
                     % about an edge program; this one is level rules only.
                     aggregate_head_shape,
                     aggregate_not_implemented,
                     % After the two classes about the aggregate WORD itself:
                     % this one is about the column that word reads, and only
                     % has a question to ask once the word is one both doors
                     % evaluate.
                     aggregate_operand_not_number,
                     finalize_in_level_rule,
                     latest_in_level_rule,
                     pre_in_level_rule ]).

check_program(Program) :-
    engine_check_order(Order),
    (   first_violation(Program, Order, violation(Name, Payload))
    ->  engine_unsupported(Name, Payload, Term),
        throw(Term)
    ;   clock_violation(Program, ClockViolation)
    ->  throw(ClockViolation)
    ;   recursion_refusal(Program, Term)
    ->  throw(Term)
    ;   true
    ).

% Oracle twin of lower.pl's recursion throws (5205/5260/5264): both doors must
% throw the same term on the same direct recursive spelling (PR #266 class).
recursion_refusal(prog(_Decls, Rules), Term) :-
    member(Rule, Rules),
    Rule = (Head <- _Body),
    rel_ref(Head, Ref),
    self_read_count(Rule, Ref, Count),
    Count >= 1,
    (   Count =\= 1
    ->  Term = unsupported_construct(
                  recursive_cte_multiple_self_reads(Ref, Count))
    ;   recursive_head_text_build(Head)
    ->  Term = unsupported_construct(built_text_in_recursive_head(Ref))
    ;   recursive_head_list_build(Head)
    ->  Term = unsupported_construct(built_list_in_recursive_head(Ref))
    ;   fail
    ).

self_read_count((_ <- Body), Ref, Count) :-
    body_atoms(Body, Atoms),
    include(reads_ref(Ref), Atoms, SelfAtoms),
    length(SelfAtoms, Count).

reads_ref(Ref, Atom) :- rel_ref(Atom, Ref).

recursive_head_text_build(Head) :-
    Head =.. [_ | Args],
    member(Arg, Args),
    compound(Arg),
    text_build_expr(Arg).

text_build_expr(concat(_)) :- !.
text_build_expr(Expr) :-
    functor(Expr, Functor, Arity),
    memberchk(Functor/Arity,
              [ norm/1, upper/1, lower/1, trim/1, trim/2,
                ltrim/1, ltrim/2, rtrim/1, rtrim/2,
                reverse/1, replace/3, initcap/1, substr/2, substr/3 ]).

recursive_head_list_build(Head) :-
    Head =.. [_ | Args],
    member(Arg, Args),
    compound(Arg),
    functor(Arg, split, 2).

engine_unsupported(type_cycle,              Names, type_cycle(Names)).
engine_unsupported(relation_pattern_not_a_relation_value,
               pattern(Ref, Column, TypeName, Value),
               relation_pattern_not_a_relation_value(Ref, Column, TypeName, Value)).
engine_unsupported(dynamic_relation_name, Ref, dynamic_relation_name(Ref)).
engine_unsupported(reserved_relation_value_carrier, Ref,
                   reserved_relation_value_carrier(Ref)).
% The oracle uses one unsupported construct term for all reserved body words.
engine_unsupported(reserved_body_word, reserved(Ref, _), reserved_body_word(Ref)).
engine_unsupported(relation_value_under_negation,
               pattern(Ref, Column, TypeName, Value),
               relation_value_under_negation(Ref, Column, TypeName, Value)).
engine_unsupported(relation_value_in_edge_rule,
               pattern(Ref, Column, TypeName, Value),
               relation_value_in_edge_rule(Ref, Column, TypeName, Value)).
engine_unsupported(relation_column_type_conflict,
               conflict(Ref, Column, TypeName, OtherRef, OtherColumn, OtherType),
               relation_column_type_conflict(Ref, Column, TypeName,
                                             OtherRef, OtherColumn, OtherType)).
% Same term at both doors. The payload reads head-first, because the head is
% the column the author has to change.
engine_unsupported(head_column_type_conflict,
               conflict(HeadRef, HeadColumn, HeadType,
                        BodyRef, BodyColumn, BodyType),
               head_column_type_conflict(HeadRef, HeadColumn, HeadType,
                                         BodyRef, BodyColumn, BodyType)).
engine_unsupported(cst_capture_unused, Name, cst_capture_unused(Name)).
engine_unsupported(cst_variable_uncaptured, Name,
               cst_variable_uncaptured(Name)).
engine_unsupported(regexp_pattern_not_literal, Payload, Payload).
engine_unsupported(regexp_pattern_outside_subset, Payload, Payload).
engine_unsupported(regexp_pattern_invalid, Payload, Payload).
engine_unsupported(regexp_operand_not_text, Payload, Payload).
engine_unsupported(column_type_unknown,     Name,  column_type_unknown(Name)).
engine_unsupported(key_position_out_of_range, Payload, Payload).
engine_unsupported(key_position_duplicate,    Payload, Payload).
engine_unsupported(keyed_level_head,        Ref,   keyed_level_head(Ref)).
engine_unsupported(keyed_log_rel,           Ref-_, keyed_log_rel(Ref)).
engine_unsupported(log_on_level_headed_rel, Ref,   log_on_level_headed_rel(Ref)).
engine_unsupported(missing_retention,       Ref,   missing_retention(Ref)).
engine_unsupported(keep_on_non_log_rel,     Ref,   keep_on_non_log_rel(Ref)).
engine_unsupported(retention_head_conflict_risk, Ref-count(N),
               retention_head_conflict_risk(Ref, count(N))).
% The oracle names this one without a reference, and always has.
engine_unsupported(aggregate_in_edge_head,  _,     aggregate_in_edge_head).
engine_unsupported(aggregate_head_shape,     Shape, aggregate_head_shape(Shape)).
engine_unsupported(aggregate_not_implemented,
               unimplemented(Ref, Signature, Implemented),
               aggregate_not_implemented(Ref, Signature, Implemented)).
% Same NAME as lower.pl's own unsupported construct, different payload, the way keyed_log_rel
% already differs by door. The compiler's middle argument is the compiled
% operand EXPRESSION, which for a plain column variable is an unbound variable
% and names nothing; this door has the declared column in hand and says which
% one it is.
engine_unsupported(aggregate_operand_not_number,
               operand(Kind, Ref, Column, Type),
               aggregate_operand_not_number(Kind, Ref, Column, Type)).
engine_unsupported(finalize_in_level_rule,  Ref,   finalize_in_level_rule(Ref)).
engine_unsupported(latest_in_level_rule,    Ref,   latest_in_level_rule(Ref)).
engine_unsupported(pre_in_level_rule,       Ref,   pre_in_level_rule(Ref)).

% ═══ the store ══════════════════════════════════════════════════════════════
% srow(Row) for Set rels; lrow(st(Tick, Seq), Row) for Log rels. Level views
% are computed, never stored.

% Multiset: a Log rel's duplicate rows are distinct occurrences and stay
% visible (store dedup would silently re-collapse what q1 preserves).
% Direct recursion, not findall/member: this runs once per occurrence over the
% whole store, so the findall bag was the single largest allocation in a tick.
store_rows(Store, Rows) :-
    entry_rows(Store, Rows0),
    msort(Rows0, Rows).

entry_rows([], []).
entry_rows([srow(Row) | Entries], [Row | Rows]) :- entry_rows(Entries, Rows).
entry_rows([lrow(_, Row) | Entries], [Row | Rows]) :- entry_rows(Entries, Rows).

log_stamps(Store, Ref, Stamps) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store), rel_ref(Row, Ref) ), Stamps0),
    msort(Stamps0, Stamps).

% Bare positive atoms are trigger sources; latest(Atom) is a sampled read.
% r4: finalize(Atom) is a DEPARTURE trigger position; it fires on a Set/level
% row's -delta arriving as a next-tick occurrence, and is never satisfiable
% as a read (the row is gone). Items are arrival(Atom) | departure(Atom).
% The conjunction spine comes from the shared walk; classification stays here.
%
% The walk must NOT descend not/1: a negated atom is not a trigger.
%
% next/1 and combine are transparent splice rows and must contribute trigger
% items from their component goals.
%
% Every body WITHOUT a splice row walks identically under either policy --
% walk_children/7 branches on splice_bare only for a splice_bare surface row --
% so this widens nothing else in the corpus.
trigger_items(Body, Items) :-
    walk_body(Body, walk_policy(descend_not(false), splice_bare(true)),
              Events),
    trigger_items_(Events, Items).

trigger_items_([], []).
trigger_items_([event(_, _, Surface, Term) | Rest], Items) :-
    (   nonvar(Term), Term = finalize(Atom)
    ->  Items = [departure(Atom) | RestItems]
    % Bound directly out of the walked body, NEVER copied: a copy would sever
    % the trigger atom from the body and let solve rejoin over the whole store.
    ;   Surface == plain_atom
    ->  Items = [arrival(Term) | RestItems]
    ;   Items = RestItems
    ),
    trigger_items_(Rest, RestItems).

wrap_arrival(Atom, arrival(Atom)).

% An occurrence either is a departure (dep(Row) payload) matching a
% departure item, with the ground finalize goal substituted away before
% solving (the row is absent from Visible), or a plain arrival.
occurrence_trigger(dep(Row), Items, Body0, Body) :- !,
    member(departure(Atom), Items), Atom = Row,
    substitute_goal(Body0, finalize(Row), Body).
occurrence_trigger(Row, Items, Body, Body) :-
    member(arrival(Atom), Items), Atom = Row.


listened_departure_refs(Rules, Refs) :-
    findall(Ref, ( member((_ <+ Body), Rules), body_finalize_ref(Body, Ref) ),
            Refs0),
    sort(Refs0, Refs).

% finalize/1 does not descend not/1: a negated finalize is not a departure.
body_finalize_ref(Body, Ref) :-
    body_wrapper_refs(Body, finalize,
                      walk_policy(descend_not(false), splice_bare(false)),
                      Ref).

body_latest_ref(Body, Ref) :-
    body_wrapper_refs(Body, latest,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

body_pre_ref(Body, Ref) :-
    body_wrapper_refs(Body, pre,
                      walk_policy(descend_not(true), splice_bare(false)),
                      Ref).

% ═══ arrivals ═══════════════════════════════════════════════════════════════

absorb_arrivals(_, _, [], Store, Seq, Store, Seq, []).
absorb_arrivals(Prog, Tick, [Signed | Rest], Store0, Seq0, Store, Seq, Occurrences) :-
    Prog = prog(Decls, _),
    (   Signed = +Row
    ->  rel_ref(Row, Ref),
        rel_kind(Decls, Ref, Kind),
        (   Kind == log
        ->  Store1 = [lrow(st(Tick, Seq0), Row) | Store0],
            Seq1 is Seq0 + 1,
            Occurrences = [occ(st(Tick, Seq0), Row) | More]
        ;   absorb_set_arrival(Decls, Row, Store0, Store1, Changed),
            ( Changed == false
            -> Occurrences = More, Seq1 = Seq0
            ;  check_parent_chain(Decls, Row, Store1),
               Seq1 is Seq0 + 1,
               Occurrences = [occ(st(Tick, Seq0), Row) | More] ) )
    ;   Signed = -Row,
        rel_ref(Row, Ref),
        rel_kind(Decls, Ref, Kind),
        ( Kind == log -> throw(retract_from_log(Ref)) ; true ),
        exclude(==(srow(Row)), Store0, Store1),
        Seq1 = Seq0, Occurrences = More
    ),
    absorb_arrivals(Prog, Tick, Rest, Store1, Seq1, Store, Seq, More).

% The emitted door's BEFORE INSERT trigger, walked in prolog: out-degree is 1
% by the companion's key, so the chain from the arriving row is a simple path.
check_parent_chain(Decls, Row, Store) :-
    rel_ref(Row, Ref),
    (   acyclic_companion(Decls, Ref, _, _, _),
        Row =.. [_, Node, Parent],
        parent_chain_to(Store, Ref, Node, Parent, [Node], Path)
    ->  throw(parent_cycle(Node, path(Path)))
    ;   true
    ).

parent_chain_to(Store, Ref, Node, Parent, Seen, Path) :-
    (   Parent == Node
    ->  reverse([Parent | Seen], Path)
    ;   \+ memberchk(Parent, Seen),
        parent_edge(Store, Ref, Parent, Grandparent),
        parent_chain_to(Store, Ref, Node, Grandparent, [Parent | Seen], Path)
    ).

parent_edge(Store, Name/2, From, To) :-
    Row =.. [Name, From, To],
    memberchk(srow(Row), Store).

absorb_set_arrival(_, Row, Store, Store, false) :-
    memberchk(srow(Row), Store),
    !.
absorb_set_arrival(Decls, Row, Store0, [srow(Row) | Kept], true) :-
    rel_ref(Row, Ref),
    decl_key(Decls, Ref, Positions),
    key_of(Positions, Row, Key),
    select(srow(Old), Store0, Kept),
    rel_ref(Old, Ref),
    key_of(Positions, Old, Key),
    !.
absorb_set_arrival(_, Row, Store, [srow(Row) | Store], true).

% ═══ edge firing, one occurrence at a time ══════════════════════════════════

% The trigger items are a property of the rule, not of the occurrence, so they
% are walked once per tick. rule(Head, Body, Items) is copied as ONE term: the
% items must stay bound into the body copy they came from (see trigger_items_).
process_occurrences(Prog, Tick, Frozen, Occurrences, Store0, Store, Written) :-
    Prog = prog(_, Rules),
    findall(rule(Head, Body, Items),
            ( member((Head <+ Body), Rules), trigger_items(Body, Items) ),
            Edges),
    process_occurrences_(Occurrences, Prog, Tick, Frozen, Edges, none, Store0, Store, Written).

% An occurrence that writes nothing leaves the store term identical, and most
% do, so the two whole-store views are rebuilt only when the store changed.
% The guard is ==/2 on the store term itself, not on its rows.
store_view(Store, _, view(Cached, Visible, PreState), view(Cached, Visible, PreState)) :-
    Cached == Store, !.
store_view(Store, frozen(MidLevel, PrevLevel), _, view(Store, Visible, PreState)) :-
    store_rows(Store, StoreRows),
    append(StoreRows, MidLevel, Visible0), sort(Visible0, Visible),
    append(StoreRows, PrevLevel, PreState0), sort(PreState0, PreState).

process_occurrences_([], _, _, _, _, _, Store, Store, []).
process_occurrences_([occ(_, Payload) | Rest], Prog, Tick, Frozen, Edges,
                     View0, Store0, Store, Written) :-
    Prog = prog(Decls, _),
    store_view(Store0, Frozen, View0, View),
    View = view(_, Visible, PreState),
    findall(EvaluatedHead,
            ( member(Edge, Edges),
              copy_term(Edge, rule(HeadCopy, BodyCopy, Items)),
              occurrence_trigger(Payload, Items, BodyCopy, SolvableBody),
              solve(SolvableBody, ctx(Visible, PreState, Tick)),
              eval_head(HeadCopy, EvaluatedHead) ),
            Derived0),
    dedupe_keep_order(Derived0, Derived),
    check_occurrence_conflicts(Decls, Derived),
    apply_edge_writes(Prog, Tick, Derived, Store0, Store1, WrittenHere),
    process_occurrences_(Rest, Prog, Tick, Frozen, Edges, View, Store1, Store, WrittenRest),
    append(WrittenHere, WrittenRest, Written).

check_occurrence_conflicts(Decls, Derived) :-
    forall(( member(Row, Derived), rel_ref(Row, Ref),
             decl_key(Decls, Ref, Positions), key_of(Positions, Row, Key) ),
           ( findall(Other, ( member(Other, Derived), rel_ref(Other, Ref),
                              key_of(Positions, Other, Key) ), Others0),
             sort(Others0, Others),
             ( Others = [_] -> true ; throw(keyed_conflict(Ref, Key, Others)) ) )).

apply_edge_writes(_, _, [], Store, Store, []).
apply_edge_writes(Prog, Tick, [Row | Rest], Store0, Store, Written) :-
    Prog = prog(Decls, _),
    rel_ref(Row, Ref),
    (   rel_kind(Decls, Ref, log)
    ->  next_seq(Store0, Tick, Seq),
        Store1 = [lrow(st(Tick, Seq), Row) | Store0],
        Written = [Row | More]
    ;   decl_key(Decls, Ref, Positions)
    ->  key_of(Positions, Row, Key),
        (   member(srow(Old), Store0), rel_ref(Old, Ref), key_of(Positions, Old, Key)
        ->  (   Old == Row
            ->  Store1 = Store0, Written = More         % equal-row write = no-op
            ;   exclude(==(srow(Old)), Store0, Kept),
                Store1 = [srow(Row) | Kept], Written = [Row | More] )
        ;   Store1 = [srow(Row) | Store0], Written = [Row | More] )
    ;   throw(edge_into_unkeyed_set(Ref))
    ),
    apply_edge_writes(Prog, Tick, Rest, Store1, Store, More).

next_seq(Store, Tick, Seq) :-
    findall(Number, member(lrow(st(Tick, Number), _), Store), Numbers),
    ( max_list(Numbers, Max) -> Seq is Max + 1 ; Seq = 1 ).

% ═══ retention (q10) ════════════════════════════════════════════════════════

apply_retention(prog(Decls, _), Store0, Store) :-
    findall(Ref-Bound, member(keep(Ref, Bound), Decls), Bounds),
    foldl(prune_rel, Bounds, Store0, Store).

prune_rel(_-all, Store, Store) :- !.
prune_rel(Ref-count(Limit), Store0, Store) :-
    log_stamps(Store0, Ref, Stamped),
    length(Stamped, Total),
    Drop is max(0, Total - Limit),
    length(Dropped, Drop), append(Dropped, _, Stamped),
    exclude(dropped_entry(Ref, Dropped), Store0, Store).

dropped_entry(Ref, Dropped, lrow(Stamp, Row)) :-
    rel_ref(Row, Ref), memberchk(Stamp-Row, Dropped).

% ═══ THE TICK ═══════════════════════════════════════════════════════════════
% state(Tick, Store, PrevLevel, PrevAll)

tick(Prog, state(Tick, Store0, PrevLevel, PrevAll), CarryIn, OutsideArrivals,
     state(NextTick, Store, Level, NextAll), CarryOut, Deltas) :-
    Prog = prog(TickDecls, Rules),
    split_rules(Rules, AggRules, PlainLevel, _),
    absorb_arrivals(Prog, Tick, OutsideArrivals, Store0, 1, StoreArrived, _, ArrivalOccs),
    store_rows(StoreArrived, MidBase),
    level_closure(TickDecls, PlainLevel, AggRules, MidBase, Tick, MidLevel),
    ord_subtract(MidLevel, PrevLevel, NewLevelRows),
    stamp_extra(Tick, NewLevelRows, 1000, LevelOccs),
    stamp_extra(Tick, CarryIn, 2000, CarryOccs),
    append([CarryOccs, ArrivalOccs, LevelOccs], Occurrences),
    process_occurrences(Prog, Tick, frozen(MidLevel, PrevLevel), Occurrences,
                        StoreArrived, StoreWritten, WrittenRows),
    apply_retention(Prog, StoreWritten, Store),
    store_rows(Store, FinalBase),
    level_closure(TickDecls, PlainLevel, AggRules, FinalBase, Tick, Level),
    ord_subtract(Level, MidLevel, PostWriteLevelRows),
    append(FinalBase, Level, NextAll0), sort(NextAll0, NextAll),
    boundary_deltas(Prog, Store0, Store, PrevAll, NextAll, Deltas),
    % R2 rider + R7: carry-out is boundary-observable writes only. A row this
    % tick wrote but that is not a +delta (intermediate fold state, net-zero
    % fold, equal-row no-op) never becomes a T+1 arrival.
    append(WrittenRows, PostWriteLevelRows, CarryCandidates0),
    dedupe_keep_order(CarryCandidates0, CarryCandidates),
    findall(Row, ( member(Row, CarryCandidates), memberchk(+Row, Deltas) ), ArrivalCarry),
    % r4: a -delta of a LISTENED rel is a departure occurrence at T+1. Only
    % rels some rule actually binds with finalize/1 carry, so programs
    % without departure rules never mint drain ticks for retractions.
    listened_departure_refs(Rules, DepartureRefs),
    findall(dep(Row),
            ( member(-Row, Deltas), rel_ref(Row, DepRef), memberchk(DepRef, DepartureRefs) ),
            DepartureCarry),
    append(ArrivalCarry, DepartureCarry, CarryOut),
    NextTick is Tick + 1.

stamp_extra(_, [], _, []).
stamp_extra(Tick, [Row | Rest], Seq, [occ(st(Tick, Seq), Row) | More]) :-
    NextSeq is Seq + 1,
    stamp_extra(Tick, Rest, NextSeq, More).

% r7: Log rels emit one +Row per new stamp and one -Row per stamp retention
% reclaimed; everything else is a set diff of the full visible state (removed
% then added).
boundary_deltas(prog(Decls, _), Store0, Store, PrevAll, NextAll, Deltas) :-
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store),
              \+ memberchk(lrow(Stamp, Row), Store0) ),
            NewStamped0),
    msort(NewStamped0, NewStamped),
    findall(+Row, member(_-Row, NewStamped), LogAdds),
    % The symmetric half of the stamp diff. A stamp present at tick start and
    % absent at tick end was reclaimed by retention, so it is reported as an
    % ordinary minus. Only apply_retention/3 can remove a stamp: a world or
    % program retraction of a Log rel throws retract_from_log/1 first, so this
    % arm is reachable exclusively through a bound the program declared.
    %
    % R7 is not weakened. A minus here is a STORAGE-plane fact (the row was
    % reclaimed), never an occurrence-plane one (the firing still happened and
    % every rule that was going to see it already did). See the storage-vs-
    % occurrence statement in compile/TICK-MODEL.md section 5.
    findall(Stamp-Row,
            ( member(lrow(Stamp, Row), Store0),
              \+ memberchk(lrow(Stamp, Row), Store) ),
            GoneStamped0),
    msort(GoneStamped0, GoneStamped),
    findall(-Row, member(_-Row, GoneStamped), LogRemovals),
    % PrevAll and NextAll are ordsets (both sort/2 output, tick/7 and
    % run_program/5), so the difference is a linear merge in the same order.
    ord_subtract(PrevAll, NextAll, GoneRows),
    findall(-Row, ( member(Row, GoneRows), delta_ref_is_set(Decls, Row) ),
            SetRemovals),
    ord_subtract(NextAll, PrevAll, NewRows),
    findall(+Row, ( member(Row, NewRows), delta_ref_is_set(Decls, Row) ),
            SetAdds),
    append([SetRemovals, LogRemovals, SetAdds, LogAdds], Deltas).

delta_ref_is_set(Decls, Row) :-
    rel_ref(Row, Ref), rel_kind(Decls, Ref, Kind), Kind == set.

% ═══ the run loop, engine-owned drains (q5) ═════════════════════════════════

run_program(SugaredProg, Initial0, Schedule0, FinalAll, DeltaTicks) :-
    list_mint_reset,
    prepare_program(SugaredProg, HostProg, _, _, _),
    % Host preparation stays a PRE-PASS: it mixes syntax normalization with
    % world-plan extraction, so it does not belong in the four-phase table.
    % Everything after it runs in the declared order (1_expansion.pl).
    expand_program(HostProg, ExpandedProg, _),
    check_program(ExpandedProg),
    % A relation-shaped TERM in a rule is the surface spelling of a relation
    % value; obj(SortedPairs) is the value. Rewriting here, after the shape
    % unsupported construct and before anything stores or unifies, is what makes a
    % rule-BUILT value and a world-ARRIVED value the same term at every depth
    % (0_relation_pattern.pl).
    expand_relation_values(ExpandedProg, Prog),
    check_world_shapes(Prog, Initial0, Schedule0),
    % struct_arrival_key_order ruling: arrival key order is insignificant --
    % the decl induces the canonical form, so every world row is rewritten to
    % it HERE, before any store or Set membership can see a second spelling.
    Prog = prog(ProgDecls, _),
    canonicalize_world_rows(ProgDecls, Initial0, CanonicalInitial),
    maplist(canonicalize_world_rows(ProgDecls), Schedule0, CanonicalSchedule),
    normalize_relation_reference_rows(ProgDecls, CanonicalInitial, Initial),
    maplist(normalize_relation_reference_rows(ProgDecls), CanonicalSchedule, Schedule),
    seed_store(Prog, Initial, Store0),
    Prog = prog(_, Rules),
    % PARITY PREP: the same cone the compiler threads into every emitted
    % module (compile.pl:program_plan/2), computed from the same shared module
    % on the same post-expansion program. The oracle asserts nothing with it
    % yet; computing it here is what makes a later parity check a diff of two
    % numbers rather than a new analysis on one side.
    findall(QueryAtom,
            ( member(QueryDecl, ProgDecls), query_decl(QueryDecl, QueryAtom, _) ),
            Queries),
    subscribed_rels(ProgDecls, Rules, Queries, _SubscribedRels),
    split_rules(Rules, AggRules, PlainLevel, _),
    store_rows(Store0, BaseRows),
    level_closure(ProgDecls, PlainLevel, AggRules, BaseRows, 0, Level0),
    append(BaseRows, Level0, All0), sort(All0, PrevAll),
    run_ticks(Prog, state(1, Store0, Level0, PrevAll), [], Schedule, 0, FinalAll, DeltaTicks).

% SLOT-ARRIVAL-MALFORMED (ruling compound_storage = struct_as_rows): a world
% row whose value does not match the declared struct shape is a NAMED unsupported construct
% at the boundary. The check is decl-driven -- a row that passes runs exactly
% as it did before the type existed -- and it runs here, where both the seed
% rows and the whole schedule are in hand, rather than inside absorb_arrivals,
% so a malformed row is a load failure and never a half-applied tick.
check_world_shapes(prog(Decls, _), Initial, Schedule) :-
    append([Initial | Schedule], WorldRows),
    (   world_row_shape_violation(Decls, WorldRows, mismatch(Ref, Column, TypeName, Reason))
    ->  ( Reason = int_out_of_range(Value)
        -> throw(int_out_of_range(Ref, Column, Value))
        ;  throw(type_arrival_shape_mismatch(Ref, Column, TypeName, Reason))
        )
    ;   true
    ).

seed_store(prog(Decls, _), Initial, Store) :-
    findall(Entry,
            ( nth1(Position, Initial, Row),
              rel_ref(Row, Ref),
              ( rel_kind(Decls, Ref, log)
              -> Entry = lrow(st(0, Position), Row)
              ;  Entry = srow(Row) ) ),
            Store).

run_ticks(prog(Decls, Rules), state(_, Store, Level, _), [], [], _, FinalAll, []) :- !,
    store_rows(Store, Rows),
    append(Rows, Level, FinalAll0), msort(FinalAll0, FinalAll1),
    list_boundary_rows(Decls, Rules, FinalAll1, FinalAll).
run_ticks(Prog, State, Carry, [Arrivals | Schedule], Drains, FinalAll, [Rendered | More]) :- !,
    tick(Prog, State, Carry, Arrivals, NextState, NextCarry, Deltas),
    tick_boundary_deltas(Prog, NextState, Deltas, Rendered),
    run_ticks(Prog, NextState, NextCarry, Schedule, Drains, FinalAll, More).
run_ticks(Prog, State, Carry, [], Drains, FinalAll, [Rendered | More]) :-
    Carry \== [],
    drain_cap(Cap),
    ( Drains >= Cap -> throw(drain_overflow(Cap)) ; true ),
    NextDrains is Drains + 1,
    tick(Prog, State, Carry, [], NextState, NextCarry, Deltas),
    tick_boundary_deltas(Prog, NextState, Deltas, Rendered),
    run_ticks(Prog, NextState, NextCarry, [], NextDrains, FinalAll, More).

% Post-tick rows, because the emitted door's boundary read runs after every
% write of the tick and reads the member rel as it then stands.
tick_boundary_deltas(prog(Decls, Rules), state(_, Store, Level, _), Deltas,
                     Rendered) :-
    store_rows(Store, Rows),
    append(Rows, Level, Visible),
    list_boundary_deltas(Decls, Rules, Visible, Deltas, Rendered).

dedupe_keep_order([], []).
dedupe_keep_order([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe_keep_order(Filtered, Out).

% ═══ fixture running ════════════════════════════════════════════════════════
% fixture(Name, prog(Decls, Rules), InitialRows, Schedule, Expectations)
% Expectations: final(Ref, SortedRows) | deltas(Ref, PerTick) | ticks(N)
%             | throws(Term)

rel_rows(Ref, Rows, Selected) :-
    findall(Row, ( member(Row, Rows), rel_ref(Row, Ref) ), Selected0),
    msort(Selected0, Selected).

rel_deltas(Ref, DeltaTicks, Selected) :- maplist(rel_delta_tick(Ref), DeltaTicks, Selected).

rel_delta_tick(Ref, Deltas, Selected) :-
    findall(Delta, ( member(Delta, Deltas), delta_row(Delta, Row), rel_ref(Row, Ref) ),
            Selected).

delta_row(+Row, Row).
delta_row(-Row, Row).

fixture_expectations_hold(Name, Expectations) :-
    user:fixture(Name, Prog, Initial, Schedule, Expectations),
    (   memberchk(throws(Expected), Expectations)
    ->  catch((run_program(Prog, Initial, Schedule, _, _), fail), Thrown, true),
        Thrown == Expected
    ;   run_program(Prog, Initial, Schedule, FinalAll, DeltaTicks),
        forall(member(Expectation, Expectations),
               expectation_holds(Expectation, FinalAll, DeltaTicks))
    ).

expectation_holds(final(Ref, Expected), FinalAll, _) :-
    rel_rows(Ref, FinalAll, Actual),
    ( Actual == Expected -> true
    ; format("    MISMATCH final ~w~n      got ~q~n      want ~q~n", [Ref, Actual, Expected]),
      fail ).
expectation_holds(deltas(Ref, Expected), _, DeltaTicks) :-
    rel_deltas(Ref, DeltaTicks, Actual),
    ( Actual == Expected -> true
    ; format("    MISMATCH deltas ~w~n      got ~q~n      want ~q~n", [Ref, Actual, Expected]),
      fail ).
expectation_holds(ticks(Expected), _, DeltaTicks) :-
    length(DeltaTicks, Actual),
    ( Actual == Expected -> true
    ; format("    MISMATCH ticks got ~w want ~w~n", [Actual, Expected]), fail ).

run_fixture_checks(Name, Goal) :-
    user:fixture(Name, _, _, _, Expectations),
    Goal = engine:fixture_expectations_hold(Name, Expectations).
