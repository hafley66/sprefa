% Generic scan instantiation against the current V6 program IR.
%
% Run:
%   swipl -q -l v6/prolog/labs/generic_scan_instantiation/0_receipts.pl \
%     -g go -g halt
%
% This lab adds no parser, compiler, runtime, or surface syntax. scan_spec/6 is
% compiler metadata. specialize_scan/3 erases it into the current prog/2 IR.

:- module(generic_scan_instantiation_receipts,
          [ go/0,
            scan_spec/6,
            scan_plan_fact/3,
            specialize_scan/3,
            scan_unsupported/2
          ]).

:- use_module('../../conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../compile', [program_plan/2]).
:- use_module('../../lower', [lower_program/2]).
:- use_module('../../compile/registry', [expression/5]).
:- use_module(library(crypto), [crypto_data_hash/3]).
:- use_module(library(lists), [list_to_set/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous step_sig/6.
:- discontiguous step_rule/3.

% Program objects used by the lab.
%
% rel_def(Ref, Kind, Columns, Types, Key).

rel_def(add/3, log, [key, event_id, amount], [text, text, int], none).
rel_def(subtract/3, log, [key, event_id, amount], [text, text, int], none).
rel_def(text_add/3, log, [key, event_id, amount], [text, text, text], none).
rel_def(total_seed/2, log, [key, value], [text, int], none).
rel_def(other_seed/2, log, [key, value], [text, int], none).
rel_def(shared_seed/2, log, [key, value], [text, int], none).
rel_def(total/2, set, [key, value], [text, int], key([1])).
rel_def(other_total/2, set, [key, value], [text, int], key([1])).
rel_def(shared_total/2, set, [key, value], [text, int], key([1])).

rel_def(child_add/4, log,
        [owner, child, event_id, amount],
        [text, text, text, int],
        none).
rel_def(child_seed/3, log,
        [owner, child, value],
        [text, text, int],
        none).
rel_def(child_total/3, set,
        [owner, child, value],
        [text, text, int],
        key([1, 2])).

% Step relations are compile-time rule sets. The current runtime never stores
% rows for these refs.

step_sig(add_step/3,
         slots([typevar(state), typevar(event), typevar(state)]),
         constraints([state-int, event-int]),
         grade(0),
         cardinality(det),
         effects([])).
step_rule(add_step/3, add_step(Previous, Amount, Next),
          (Next := Previous + Amount)).

step_sig(subtract_step/3,
         slots([typevar(state), typevar(event), typevar(state)]),
         constraints([state-int, event-int]),
         grade(0),
         cardinality(det),
         effects([])).
step_rule(subtract_step/3, subtract_step(Previous, Amount, Next),
          (Next := Previous - Amount)).

step_sig(ambiguous_step/3,
         slots([typevar(state), typevar(event), typevar(state)]),
         constraints([state-int, event-int]),
         grade(0),
         cardinality(multi),
         effects([])).
step_rule(ambiguous_step/3, ambiguous_step(Previous, Amount, Next),
          (Next := Previous + Amount)).
step_rule(ambiguous_step/3, ambiguous_step(Previous, Amount, Next),
          (Next := Previous - Amount)).

step_sig(recursive_step/3,
         slots([typevar(state), typevar(event), typevar(state)]),
         constraints([state-int, event-int]),
         grade(0),
         cardinality(unknown),
         effects([])).
step_rule(recursive_step/3, recursive_step(Previous, Amount, Next),
          recursive_step(Previous, Amount, Next)).

% key_map(
%   event(KeyPositions, PayloadPositions),
%   state(KeyPositions, ValuePositions),
%   init(KeyPositions, ValuePositions),
%   step(PreviousPositions, EventPositions, NextPositions)).

scalar_key_map(
    key_map(event([1], [3]),
            state([1], [2]),
            init([1], [2]),
            step([1], [2], [3]))).

child_key_map(
    key_map(event([1, 2], [4]),
            state([1, 2], [3]),
            init([1, 2], [3]),
            step([1], [2], [3]))).

scan_spec(total_scan, add/3, total/2, total_seed/2, add_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(total_scan_alias, add/3, total/2, total_seed/2, add_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(other_total_scan, add/3, other_total/2, other_seed/2,
          add_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(shared_add_scan, add/3, shared_total/2, shared_seed/2,
          add_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(shared_subtract_scan, subtract/3, shared_total/2, shared_seed/2,
          subtract_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(child_scan, child_add/4, child_total/3, child_seed/3,
          add_step/3, Map) :-
    child_key_map(Map).
scan_spec(text_type_mismatch, text_add/3, total/2, total_seed/2,
          add_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(multi_reducer_scan, add/3, total/2, total_seed/2,
          ambiguous_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(recursive_reducer_scan, add/3, total/2, total_seed/2,
          recursive_step/3, Map) :-
    scalar_key_map(Map).
scan_spec(unknown_init_scan, add/3, total/2, unknown_seed/2,
          add_step/3, Map) :-
    scalar_key_map(Map).

% Relational planner facts. Datalog can derive these without constructing AST
% terms or predicate names.

scan_plan_fact(Name, role(event), EventRef) :-
    scan_spec(Name, EventRef, _, _, _, _).
scan_plan_fact(Name, role(state), StateRef) :-
    scan_spec(Name, _, StateRef, _, _, _).
scan_plan_fact(Name, role(init), InitRef) :-
    scan_spec(Name, _, _, InitRef, _, _).
scan_plan_fact(Name, role(step), StepRef) :-
    scan_spec(Name, _, _, _, StepRef, _).
scan_plan_fact(Name, storage(existing(StateRef)), persistent_tables(0)) :-
    scan_spec(Name, _, StateRef, _, _, _),
    rel_def(StateRef, set, _, _, key(_)).
scan_plan_fact(Name, clock(reduce(0)), clock(write(0), observe(1))) :-
    scan_spec(Name, _, _, _, StepRef, _),
    step_sig(StepRef, _, _, grade(0), _, effects([])).
scan_plan_fact(Name, types(Bindings), signature(StateTypes, EventTypes)) :-
    scan_type_bindings(Name, Bindings, StateTypes, EventTypes).
scan_plan_fact(Name, specialization(Hash), helper(Helper)) :-
    scan_specialization_hash(Name, Hash),
    helper_name(Hash, Helper).

% Static unsupported constructs. Ground relation names are the compile-time boundary.

scan_unsupported(scan_request(_, EventRef, _, _, _, _),
             dynamic_relation_name(event)) :-
    \+ ground(EventRef),
    !.
scan_unsupported(scan_request(_, _, StateRef, _, _, _),
             dynamic_relation_name(state)) :-
    \+ ground(StateRef),
    !.
scan_unsupported(scan_request(_, _, _, InitRef, _, _),
             dynamic_relation_name(init)) :-
    \+ ground(InitRef),
    !.
scan_unsupported(scan_request(_, _, _, _, StepRef, _),
             dynamic_relation_name(step)) :-
    \+ ground(StepRef),
    !.
scan_unsupported(Name, unknown_relation(Role, Ref)) :-
    scan_plan_fact(Name, role(Role), Ref),
    memberchk(Role, [event, state, init]),
    \+ rel_def(Ref, _, _, _, _),
    !.
scan_unsupported(Name, unknown_step(StepRef)) :-
    scan_plan_fact(Name, role(step), StepRef),
    \+ step_sig(StepRef, _, _, _, _, _),
    !.
scan_unsupported(Name, state_requires_key(StateRef)) :-
    scan_spec(Name, _, StateRef, _, _, _),
    rel_def(StateRef, _, _, _, Key),
    Key \= key(_),
    !.
scan_unsupported(Name, recursive_reducer(StepRef)) :-
    scan_spec(Name, _, _, _, StepRef, _),
    step_reaches_itself(StepRef),
    !.
scan_unsupported(Name, reducer_cardinality(StepRef, Cardinality)) :-
    scan_spec(Name, _, _, _, StepRef, _),
    step_sig(StepRef, _, _, _, cardinality(Cardinality), _),
    Cardinality \== det,
    !.
scan_unsupported(Name, reducer_effect(StepRef)) :-
    scan_spec(Name, _, _, _, StepRef, _),
    step_sig(StepRef, _, _, _, _, effects(Effects)),
    Effects \== [],
    !.
scan_unsupported(Name, reducer_grade(StepRef, Grade)) :-
    scan_spec(Name, _, _, _, StepRef, _),
    step_sig(StepRef, _, _, grade(Grade), _, _),
    Grade =\= 0,
    !.
scan_unsupported(Name, type_mismatch(TypeError)) :-
    scan_spec(Name, _, _, _, _, _),
    catch(scan_type_bindings(Name, _, _, _),
          scan_type_error(TypeError),
          true),
    nonvar(TypeError),
    !.
scan_unsupported(Name, invalid_key_map(Reason)) :-
    scan_spec(Name, _, _, _, _, _),
    catch(validate_key_map(Name), scan_key_error(Reason), true),
    nonvar(Reason),
    !.

step_reaches_itself(StepRef) :-
    step_rule(StepRef, _, Body),
    sub_term(Term, Body),
    nonvar(Term),
    functor(Term, Name, Arity),
    StepRef = Name/Arity.

valid_scan(Name) :-
    scan_spec(Name, _, _, _, _, _),
    \+ scan_unsupported(Name, _).

% Type substitution.

scan_type_bindings(Name, Bindings, StateValueTypes, EventValueTypes) :-
    scan_spec(Name, EventRef, StateRef, InitRef, StepRef,
              key_map(event(EventKey, EventValue),
                      state(StateKey, StateValue),
                      init(InitKey, InitValue),
                      step(StepPrevious, StepEvent, StepNext))),
    rel_def(EventRef, _, _, EventTypes, _),
    rel_def(StateRef, _, _, StateTypes, _),
    rel_def(InitRef, _, _, InitTypes, _),
    selected_types(EventTypes, EventKey, EventKeyTypes),
    selected_types(StateTypes, StateKey, StateKeyTypes),
    selected_types(InitTypes, InitKey, InitKeyTypes),
    require_same_types(event_state_key, EventKeyTypes, StateKeyTypes),
    require_same_types(init_state_key, InitKeyTypes, StateKeyTypes),
    selected_types(StateTypes, StateValue, StateValueTypes),
    selected_types(InitTypes, InitValue, InitValueTypes),
    require_same_types(init_state_value, InitValueTypes, StateValueTypes),
    selected_types(EventTypes, EventValue, EventValueTypes),
    step_sig(StepRef, slots(SlotTypes), constraints(Constraints), _, _, _),
    selected_types(SlotTypes, StepPrevious, StepPreviousTypes),
    selected_types(SlotTypes, StepEvent, StepEventTypes),
    selected_types(SlotTypes, StepNext, StepNextTypes),
    bind_slot_types(StepPreviousTypes, StateValueTypes, [], B0),
    bind_slot_types(StepEventTypes, EventValueTypes, B0, B1),
    bind_slot_types(StepNextTypes, StateValueTypes, B1, B2),
    apply_constraints(Constraints, B2),
    sort(B2, Bindings).

selected_types(Types, Positions, Selected) :-
    catch(maplist(nth1_from(Types), Positions, Selected),
          error(domain_error(not_less_than_one, _), _),
          throw(scan_key_error(position_out_of_range))).

nth1_from(List, Position, Value) :-
    ( integer(Position), Position > 0, nth1(Position, List, Value)
    -> true
    ; throw(scan_key_error(position_out_of_range(Position)))
    ).

require_same_types(_, Types, Types) :- !.
require_same_types(Location, Left, Right) :-
    throw(scan_type_error(Location-Left-Right)).

bind_slot_types([], [], Bindings, Bindings) :- !.
bind_slot_types([typevar(Name) | SlotRest], [Actual | ActualRest],
                Bindings0, Bindings) :-
    !,
    bind_typevar(Name, Actual, Bindings0, Bindings1),
    bind_slot_types(SlotRest, ActualRest, Bindings1, Bindings).
bind_slot_types([fixed(Expected) | SlotRest], [Actual | ActualRest],
                Bindings0, Bindings) :-
    !,
    ( Expected == Actual
    -> bind_slot_types(SlotRest, ActualRest, Bindings0, Bindings)
    ; throw(scan_type_error(fixed-Expected-Actual))
    ).
bind_slot_types(Slots, Actuals, _, _) :-
    throw(scan_type_error(slot_arity-Slots-Actuals)).

bind_typevar(Name, Actual, Bindings, Bindings) :-
    memberchk(Name-Existing, Bindings),
    !,
    ( Existing == Actual
    -> true
    ; throw(scan_type_error(typevar(Name)-Existing-Actual))
    ).
bind_typevar(Name, Actual, Bindings, [Name-Actual | Bindings]).

apply_constraints([], _).
apply_constraints([Name-Required | Rest], Bindings) :-
    ( memberchk(Name-Actual, Bindings), Actual == Required
    -> apply_constraints(Rest, Bindings)
    ; throw(scan_type_error(constraint(Name)-Required))
    ).

validate_key_map(Name) :-
    scan_spec(Name, EventRef, StateRef, InitRef, StepRef,
              key_map(event(EventKey, EventValue),
                      state(StateKey, StateValue),
                      init(InitKey, InitValue),
                      step(StepPrevious, StepEvent, StepNext))),
    rel_def(EventRef, _, EventColumns, _, _),
    rel_def(StateRef, set, StateColumns, _, key(DeclaredStateKey)),
    rel_def(InitRef, _, InitColumns, _, _),
    step_sig(StepRef, slots(StepSlots), _, _, _, _),
    same_length(EventKey, StateKey),
    same_length(InitKey, StateKey),
    same_length(StateValue, StepPrevious),
    same_length(EventValue, StepEvent),
    same_length(StateValue, StepNext),
    StateKey == DeclaredStateKey,
    positions_exist(EventColumns, EventKey),
    positions_exist(EventColumns, EventValue),
    positions_exist(StateColumns, StateKey),
    positions_exist(StateColumns, StateValue),
    positions_exist(InitColumns, InitKey),
    positions_exist(InitColumns, InitValue),
    positions_exist(StepSlots, StepPrevious),
    positions_exist(StepSlots, StepEvent),
    positions_exist(StepSlots, StepNext),
    !.
validate_key_map(Name) :-
    throw(scan_key_error(incompatible_mapping(Name))).

positions_exist(Columns, Positions) :-
    length(Columns, Arity),
    forall(member(Position, Positions),
           (integer(Position), Position > 0, Position =< Arity)).

% Prolog compiler boundary: create fresh predicate terms and clone step rule
% bodies. The planning facts above can request these rules, but cannot emit
% their fresh-variable AST terms by ordinary Datalog derivation.

specialize_scan(Name, Signature, prog(Decls, Rules)) :-
    valid_scan(Name),
    validate_key_map(Name),
    scan_type_bindings(Name, Bindings, StateTypes, EventTypes),
    scan_spec(Name, EventRef, StateRef, InitRef, _StepRef, KeyMap),
    Signature =
        scan_signature(
            types(state(StateTypes), event(EventTypes), bindings(Bindings)),
            key(KeyMap),
            clock(reduce(0), write(0), observe(1)),
            cardinality(exactly_one_next_state),
            lifetime(keyed_state),
            effects([])),
    scan_declarations(EventRef, StateRef, InitRef, Decls),
    synthesize_init_rule(Name, InitRule),
    findall(StepRule, synthesize_step_rule(Name, StepRule), StepRules),
    Rules = [InitRule | StepRules].

synthesize_init_rule(Name, (StateAtom <+ InitAtom)) :-
    scan_spec(Name, _, StateRef, InitRef, _,
              key_map(_, state(StateKey, StateValue),
                      init(InitKey, InitValue), _)),
    fresh_atom(StateRef, StateAtom),
    fresh_atom(InitRef, InitAtom),
    atom_args(StateAtom, StateArgs),
    atom_args(InitAtom, InitArgs),
    unify_positions(StateArgs, StateKey, InitArgs, InitKey),
    unify_positions(StateArgs, StateValue, InitArgs, InitValue).

synthesize_step_rule(Name,
                     (StateNext <+ (EventAtom, pre(StatePrevious), StepBody))) :-
    scan_spec(Name, EventRef, StateRef, _, StepRef,
              key_map(event(EventKey, EventValue),
                      state(StateKey, StateValue),
                      _,
                      step(StepPrevious, StepEvent, StepNext))),
    fresh_atom(EventRef, EventAtom),
    fresh_atom(StateRef, StatePrevious),
    fresh_atom(StateRef, StateNext),
    atom_args(EventAtom, EventArgs),
    atom_args(StatePrevious, PreviousArgs),
    atom_args(StateNext, NextArgs),
    unify_positions(EventArgs, EventKey, PreviousArgs, StateKey),
    unify_positions(EventArgs, EventKey, NextArgs, StateKey),
    copy_term(StepRef, StepRefCopy),
    step_rule(StepRefCopy, StepHead, StepBody),
    atom_args(StepHead, StepArgs),
    unify_positions(PreviousArgs, StateValue, StepArgs, StepPrevious),
    unify_positions(EventArgs, EventValue, StepArgs, StepEvent),
    unify_positions(NextArgs, StateValue, StepArgs, StepNext).

fresh_atom(Name/Arity, Atom) :-
    functor(Atom, Name, Arity).

atom_args(Atom, Args) :-
    Atom =.. [_ | Args].

unify_positions(LeftArgs, LeftPositions, RightArgs, RightPositions) :-
    same_length(LeftPositions, RightPositions),
    maplist(unify_position(LeftArgs, RightArgs),
            LeftPositions, RightPositions).

unify_position(LeftArgs, RightArgs, LeftPosition, RightPosition) :-
    nth1(LeftPosition, LeftArgs, Value),
    nth1(RightPosition, RightArgs, Value).

scan_declarations(EventRef, StateRef, InitRef, Decls) :-
    refs_declarations([EventRef, StateRef, InitRef], DeclGroups),
    append(DeclGroups, Decls0),
    list_to_set(Decls0, Decls).

refs_declarations([], []).
refs_declarations([Ref | Rest], [Decls | Groups]) :-
    rel_declarations(Ref, Decls),
    refs_declarations(Rest, Groups).

rel_declarations(Ref, Decls) :-
    rel_def(Ref, Kind, Columns, Types, Key),
    maplist(col_type_decl(Ref), Columns, Types, TypeDecls),
    kind_declarations(Ref, Kind, KindDecls),
    key_declarations(Ref, Key, KeyDecls),
    append([KindDecls, KeyDecls, TypeDecls], Decls).

col_type_decl(Ref, Column, Type, col_type(Ref, Column, Type)).

kind_declarations(Ref, log, [kind(Ref, log), keep(Ref, all)]).
kind_declarations(_, set, []).

key_declarations(Ref, key(Positions), [keyed(Ref, Positions)]).
key_declarations(_, none, []).

% Definition-sensitive specialization identity. Recursive reducer definitions
% are refused by this lab; the separate SCC hash lab owns recursive hashing.

scan_specialization_hash(Name, Hash) :-
    valid_scan(Name),
    scan_spec(Name, EventRef, StateRef, InitRef, StepRef, KeyMap),
    definition_hash(EventRef, EventHash),
    definition_hash(StateRef, StateHash),
    definition_hash(InitRef, InitHash),
    definition_hash(StepRef, StepHash),
    canonical_hash(
        scan_v1(EventHash, StateHash, InitHash, StepHash, KeyMap),
        Hash).

definition_hash(Ref, Hash) :-
    rel_def(Ref, Kind, Columns, Types, Key),
    !,
    canonical_hash(rel_v1(Kind, Columns, Types, Key), Hash).
definition_hash(Ref, Hash) :-
    step_sig(Ref, Slots, Constraints, Grade, Cardinality, Effects),
    findall(rule(Head, Body), step_rule(Ref, Head, Body), Rules),
    canonical_hash(
        step_v1(Slots, Constraints, Grade, Cardinality, Effects, Rules),
        Hash).

canonical_hash(Term, Hash) :-
    copy_term(Term, Copy),
    numbervars(Copy, 0, _),
    with_output_to(atom(Text), write_canonical(Copy)),
    crypto_data_hash(Text, Hash, [algorithm(sha256)]).

helper_name(Hash, Helper) :-
    sub_atom(Hash, 0, 16, _, Prefix),
    atom_concat('__scan_', Prefix, Helper).

% Receipts.

go :-
    receipt_relational_plan,
    receipt_type_and_clock_substitution,
    receipt_arithmetic_registry,
    receipt_real_oracle,
    receipt_real_compiler,
    receipt_reuse_and_helper_name,
    receipt_separate_and_shared_state,
    receipt_nested_scan,
    receipt_missing_init,
    receipt_unsupported,
    receipt_first_order_composition,
    format("11 PASS~n").

receipt_relational_plan :-
    findall(Role-Ref, scan_plan_fact(total_scan, role(Role), Ref), Roles),
    Roles ==
        [event-(add/3), state-(total/2), init-(total_seed/2),
         step-(add_step/3)],
    scan_plan_fact(total_scan, storage(existing(total/2)),
                   persistent_tables(0)),
    format("PASS scan_spec derives relation roles and zero generated persistent tables~n").

receipt_type_and_clock_substitution :-
    scan_plan_fact(total_scan, types([event-int, state-int]),
                   signature([int], [int])),
    scan_plan_fact(total_scan, clock(reduce(0)),
                   clock(write(0), observe(1))),
    specialize_scan(total_scan, Signature, _),
    Signature =
        scan_signature(
            types(state([int]), event([int]),
                  bindings([event-int, state-int])),
            _,
            clock(reduce(0), write(0), observe(1)),
            cardinality(exactly_one_next_state),
            lifetime(keyed_state),
            effects([])),
    format("PASS concrete relation types substitute into one pure scan clock signature~n").

receipt_arithmetic_registry :-
    forall(member(Op, [+, -, *, /, mod]),
           expression(Op/2, arithmetic, _, _, both_int)),
    forall(member(Op, [<, =<, >, >=]),
           expression(Op/2, ordered_comparison, _, _, both_int)),
    expression(== / 2, identity_comparison, _, _, same_type),
    expression('\\=='/2, identity_comparison, _, _, same_type),
    format("PASS reducer arithmetic and comparisons come from the current registry~n").

receipt_real_oracle :-
    specialize_scan(total_scan, _, Program),
    run_program(
        Program,
        [],
        [[+total_seed(a, 0), +total_seed(b, 10),
          +add(a, e1, 1), +add(b, e2, 2), +add(a, e3, 3)]],
        Final,
        Deltas),
    rel_rows(total/2, Final, [total(a, 4), total(b, 12)]),
    rel_deltas(total/2, Deltas,
               [[+total(a, 4), +total(b, 12)], []]),
    format("PASS specialized scan folds ordered duplicate-capable events and partitions by key~n").

receipt_real_compiler :-
    specialize_scan(total_scan, _, Program),
    Initial = [],
    Schedule =
        [[+total_seed(a, 0), +add(a, e1, 1), +add(a, e2, 2)]],
    program_plan(
        fixture(generic_scan_total, Program, Initial, Schedule, [])-[],
        Plan),
    Plan = plan(_, prog(_, Rules), _, RelPlans, _, _, _, _, _),
    \+ contains_functor(Rules, scan_spec/6),
    \+ contains_functor(Rules, add_step/3),
    findall(Ref, member(relplan(Ref, _, _, _, _), RelPlans), Refs0),
    sort(Refs0, Refs),
    Refs == [add/3, total/2, total_seed/2],
    lower_program(Plan, Lowered),
    Lowered = lowered(_, Ddl, _, EdgeStatements, _, _, _, _),
    member(edgestmt(total/2, add/3, _, _, _, _, _, ordered_arrival),
           EdgeStatements),
    member(PreDdl, Ddl),
    sub_atom(PreDdl, _, _, _, '__pre_total'),
    \+ (member(DdlText, Ddl), sub_atom(DdlText, _, _, _, '__scan_')),
    format("PASS scan erases before the real checker and SQL lowerer; 3 named rels, 1 TEMP pre, 0 helper tables~n").

contains_functor(Term, Name/Arity) :-
    sub_term(Subterm, Term),
    nonvar(Subterm),
    compound(Subterm),
    functor(Subterm, Name, Arity).

receipt_reuse_and_helper_name :-
    scan_specialization_hash(total_scan, Hash),
    scan_specialization_hash(total_scan_alias, Hash),
    helper_name(Hash, Helper),
    atom_concat('__scan_', _, Helper),
    findall(SiteHash,
            (member(Site, [total_scan, total_scan_alias]),
             scan_specialization_hash(Site, SiteHash)),
            SiteHashes),
    sort(SiteHashes, [_]),
    format("PASS identical call sites share one definition-sensitive specialization and helper name~n").

receipt_separate_and_shared_state :-
    findall(StateRef,
            (member(Scan, [total_scan, other_total_scan]),
             scan_plan_fact(Scan, role(state), StateRef)),
            Separate0),
    sort(Separate0, Separate),
    Separate == [other_total/2, total/2],
    findall(StateRef,
            (member(Scan, [shared_add_scan, shared_subtract_scan]),
             scan_plan_fact(Scan, role(state), StateRef)),
            Shared0),
    sort(Shared0, Shared),
    Shared == [shared_total/2],
    scan_specialization_hash(shared_add_scan, AddHash),
    scan_specialization_hash(shared_subtract_scan, SubtractHash),
    AddHash \== SubtractHash,
    format("PASS explicit StateRel names select 2 separate tables or 1 shared table~n").

receipt_nested_scan :-
    specialize_scan(child_scan, _, Program),
    run_program(
        Program,
        [],
        [[+child_seed(owner, left, 0),
          +child_seed(owner, right, 10),
          +child_add(owner, left, e1, 2),
          +child_add(owner, right, e2, 3),
          +child_add(owner, left, e3, 4)]],
        Final,
        _),
    rel_rows(child_total/3, Final,
             [child_total(owner, left, 6),
              child_total(owner, right, 13)]),
    format("PASS nested scan is an explicit composite-key child StateRel~n").

receipt_missing_init :-
    specialize_scan(total_scan, _, Program),
    run_program(Program, [], [[+add(orphan, e1, 7)]], Final, _),
    rel_rows(total/2, Final, []),
    format("PASS current N-1 behavior proves the remaining gap: event before init writes zero rows~n").

receipt_unsupported :-
    scan_unsupported(unknown_init_scan,
                 unknown_relation(init, unknown_seed/2)),
    scan_unsupported(text_type_mismatch, type_mismatch(_)),
    scan_unsupported(multi_reducer_scan,
                 reducer_cardinality(ambiguous_step/3, multi)),
    scan_unsupported(recursive_reducer_scan,
                 recursive_reducer(recursive_step/3)),
    scalar_key_map(Map),
    Request =
        scan_request(dynamic_scan, EventRef, total/2, total_seed/2,
                     add_step/3, Map),
    var(EventRef),
    scan_unsupported(Request, dynamic_relation_name(event)),
    format("PASS unknown init, type mismatch, multi reducer, recursion, and dynamic rel names refuse before lowering~n").

receipt_first_order_composition :-
    Program =
        prog([],
             [ (middle(X) <- source(X)),
               (output(X) <- middle(X))
             ]),
    run_program(Program, [source(value)], [], Final, _),
    rel_rows(output/1, Final, [output(value)]),
    format("PASS A <- B already composes concrete rels; selecting B as an algorithm argument is the higher-order remainder~n").
