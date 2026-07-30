% Scan + match as a value-producing reducer.
%
% Consult:
%   swipl -q -l plans/2026-07-29-scan-match-value-lab.pl
%
% Run receipts:
%   swipl -q -l plans/2026-07-29-scan-match-value-lab.pl -g go -g halt
%
% Useful queries:
%   ?- semantic_spine(Name, Meaning).
%   ?- edge_case(Name, GeneralMatch, ScanReducer).
%   ?- choice(Question, Option, Price, Result).
%   ?- blocker(Name, Evidence, Exit).
%
% This is a lab record. It changes no parser, checker, compiler, or runtime.

:- module(scan_match_value_lab,
          [ go/0,
            goal/2,
            semantic_spine/2,
            edge_case/3,
            choice/4,
            complexity/3,
            blocker/3,
            minimal_contract/2,
            implementation_step/3
          ]).

:- use_module('../v6/prolog/0_match_expand',
              [expand_match_program_in_context/3]).
:- use_module('../v6/prolog/conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- discontiguous choice/4.

% Context
% -------
% Current match is match(SourceRel, Arms). Each arm expands into one ordinary
% rule. Current surface writes Head <- Guard or Head <+ Guard. The candidate
% direction reverses only the arm spelling:
%
%   Guard |-> ValueOrHead       level/value result
%   Guard |+> EventWrite        edge/event write
%
% An optional leading semicolon is layout sugar:
%
%   match Source (
%     ; GuardA |-> ValueA
%     ; GuardB |+> EventB
%   )
%
% General match denotes a relation. Scan adds the stricter requirement that
% the value-result relation contain exactly one derivation for each
% (scan key, queued event). The state row is named by scan, so no anonymous
% relation or runtime closure is stored.

goal(value_model,
     'make match mean a 0/1/N relational value, with no null and no implicit option').
goal(scan_model,
     'make scan consume exactly one same-clock next-state derivation per keyed queued event').
goal(lowering,
     'erase match, scan, and named reducer arguments into ordinary rels, <- rules, <+ rules, pre reads, keys, and the existing ordered event loop').
goal(reversibility,
     'hold syntax and runtime function values outside production until ghcacher and extraction prove repeated use').

semantic_spine(match,
               'Input -> Rel<Output>; arm successes union, failure is zero rows, and multiple successes are multiple derivations').
semantic_spine(none,
               'zero result rows; absence remains relational absence').
semantic_spine(some,
               'one result row; no Some wrapper is needed for an ordinary semidet query').
semantic_spine(multi,
               'two or more derivations; level storage may deduplicate equal rows, but reducer determinism counts derivations before B-set dedup').
semantic_spine(option,
               'an explicit enum none | some(Value) converts absence into one stored value when a det result is required').
semantic_spine(scan,
               'State[T,i+1] = Reducer(State[T,i], Event[T,i]); i is queue order inside one transaction tick').
semantic_spine(scan_output,
               'the keyed state relation publishes only its tick-start/tick-end B-set delta').
semantic_spine(side_output,
               '|+> writes during the transaction; its downstream trigger is carried to the next tick').
semantic_spine(higher_order,
               'the reducer name is a compile-time RuleRef; specialization erases it before checked IR and SQLite').
semantic_spine(closure,
               'an arm captures source and enclosing-rule variables; variables first bound inside an arm belong only to that arm').

% Canonical N-1 form. A future scan form only packages these declarations and
% rules. reducer/3 is pure and relational. state/2 is the keyed register.
%
%   rel state(key(Key), State).
%   rel event(Key, Event).
%
%   reducer(Previous, Event, Next) <- GuardA.
%   reducer(Previous, Event, Next) <- GuardB.
%
%   state(Key, Next) <+
%       event(Key, Event),
%       pre(state(Key, Previous)),
%       reducer(Previous, Event, Next).
%
% Future arm sugar:
%
%   GuardA |-> Next
%
% specializes to one reducer/3 rule. The scan wrapper performs the state <+
% write. A sibling `Guard |+> event(...)` specializes to an ordinary edge
% rule and does not count as the next-state value.

minimal_contract(source,
                 'one ordered occurrence source; Set snapshots are not scan inputs').
minimal_contract(identity,
                 'one declared scan key; different keys carry independent state').
minimal_contract(init,
                 'exactly one initial state exists before the first ordinary event for a key; a switch context reset is itself an ordered init event').
minimal_contract(reducer,
                 'pure grade-0 Rule<State,Event,State>, exactly one derivation per event and key').
minimal_contract(value,
                 'all |-> arms produce the declared state row shape and one common column type per position').
minimal_contract(side_write,
                 'zero or more |+> arms may stage ordinary event writes; their descendants run on a later tick').
minimal_contract(failure,
                 'missing init, zero next-state derivations, multiple next-state derivations, type mismatch, or clock mismatch aborts the whole tick').
minimal_contract(storage,
                 'keyed state rows plus ordinary event and side-output relations; no function, relation, object, or list is serialized as JSON').
minimal_contract(inference,
                 'explicit and future inferred signatures elaborate to the same types, keys, grade, cardinality, lifetime, reads, writes, and effects').

% Edge-case matrix
% ----------------

edge_case(no_arm,
          'empty relation',
          'cardinality error; spell a catch-all returning Previous when keep-state is intended').
edge_case(one_arm_one_row,
          'Some(Value)',
          'accepted as the next state').
edge_case(multiple_matching_arms,
          'union of every successful arm',
          'cardinality error even when two derivations produce an equal row').
edge_case(multi_row_arm,
          'Multi(Value) from the arm join',
          'cardinality error; aggregate or key the input before reduction').
edge_case(type_mismatch,
          'ordinary relation-head column type error',
          'compile-time refusal naming both arm result types').
edge_case(clock_mismatch,
          'arms may write separate relations on their declared clocks',
          'a next-state arm must be grade 0 at the current event position; delayed output cannot be state').
edge_case(init_order,
          'ordinary match has no initialization rule',
          'init/reset occupies a queue position; following events see it, preceding events fail the init contract').
edge_case(event_before_init,
          'a relational join currently yields zero rows',
          'strict candidate contract aborts rather than silently dropping the event').
edge_case(same_batch_init_then_event,
          'current oracle preserves the schedule list order',
          'init then event is valid and the event reads the initialized state').
edge_case(duplicate_events,
          'Log occurrences retain duplicate instances',
          'each occurrence reduces once; a Set duplicate supplies no second occurrence').
edge_case(cross_source_order,
          'match arms are unordered union',
          'merged scan sources must share one ingress sequence before reduction').
edge_case(per_key_independence,
          'ordinary joins partition through Key',
          'one total queue order defines semantics; disjoint keys may be executed in partitions later').
edge_case(context_reset,
          'keyed replacement retracts old context state',
          'reset takes effect at its queue position; later events use the new context and prior ones use the old context').
edge_case(multiple_same_tick_resets,
          'all reset occurrences run in queue order',
          'only tick-start/tick-end state is published; transient contexts start no post-commit effect').
edge_case(nested_match,
          'nested relational joins multiply cardinality',
          'the composed reducer must still prove exactly one derivation').
edge_case(nested_scan,
          'no current inline runtime graph value',
          'specialize a separately named child state keyed by parent/context identity').
edge_case(captured_variable,
          'source and enclosing variables are available to every arm',
          'capture is static specialization; no runtime closure').
edge_case(sibling_variable,
          'current evaluator shares no binding; an unbound body atom can silently widen as a wildcard',
          'alpha-rename arm locals and refuse a value/head variable not bound by source or its own arm').
edge_case(recursion,
          'grade-0 monotone B-rel recursion belongs to level fixpoint',
          'state recurrence must pass through ordered scan; reducer recursion that can produce another state candidate is refused').
edge_case(effect_in_reducer,
          'host demand/response is an ordinary graph outside a pure level rule',
          'direct host execution is refused; reducer may emit a demand relation with |+> for post-commit handling').
edge_case(delayed_event_write,
          '|+> lowers to current <+ semantics',
          'the write joins the boundary delta; descendants receive its carry on the next tick and it is not a next-state candidate').
edge_case(error,
          'ordinary rule evaluation errors abort run_program',
          'production scan must wrap all events in the existing whole-tick transaction').
edge_case(retraction,
          'keyed replacement computes -Old/+New at the boundary',
          'intermediate states are invisible to boundary observers').
edge_case(finalize,
          'finalize observes the net -Old carry one tick later',
          'multiple replacements inside one tick finalize only the tick-start endpoint; emit an explicit transition Log when every step matters').
edge_case(cache_late_result,
          'current switch lab stores result independently and gates view by active scope',
          'late result may fill cache; inactive scope produces no view row').
edge_case(enum_value,
          'constructors form ordinary values and match patterns in the oracle',
          'allowed when all arms return the declared enum; emitted compound encoding remains a blocker').
edge_case(object_value,
          'declared relational structure is rows and reference IDs',
          'return the declared row shape; no dictionary or opaque JSON state').
edge_case(list_value,
          'operational list fan-out and typed list lowering are incomplete',
          'outside the first scan contract; model collections as relations').

% Unresolved questions: at most three priced options per question.

choice(zero_next_state, strict,
       'one validation count per event or a static det proof',
       'abort tick on zero candidates').
choice(zero_next_state, implicit_keep,
       'adds a scan-only default branch',
       'zero candidates silently return Previous').
choice(zero_next_state, explicit_option,
       'requires Option<State> state and an explicit resolver',
       'none becomes data').

choice(many_next_states, strict,
       'one validation count per event or a static det proof',
       'abort tick before any candidate state is committed').
choice(many_next_states, first_arm,
       'adds ordered-choice semantics foreign to current match union',
       'lexical arm order selects one candidate').
choice(many_next_states, fold_candidates,
       'adds a second nested reduction order',
       'every candidate becomes another scan event').

choice(initialization, required_seed_or_reset,
       'validate state membership before an ordinary event',
       'seed relation or ordered context reset supplies exactly one state').
choice(initialization, lazy_seed,
       'adds a default-producing rule to every scan',
       'first event constructs missing state').
choice(initialization, silent_drop,
       'zero implementation work and matches today''s semidet join',
       'events before state disappear').

choice(cross_source_order, ingress_sequence,
       'retain one monotone queue stamp already present in the occurrence model',
       'merge preserves deterministic arrival order').
choice(cross_source_order, require_one_source,
       'no global merge contract',
       'authors must normalize sources into one event relation first').
choice(cross_source_order, unspecified,
       'zero metadata',
       'results vary with host and batch scheduling').

choice(nested_scan, named_child_state,
       'one specialized keyed relation and ordinary lifetime edge',
       'child state key includes parent/context identity').
choice(nested_scan, product_state,
       'wider parent state row and manual reducer composition',
       'both machines update in one reducer').
choice(nested_scan, runtime_graph_value,
       'closures, dynamic graph lifetime, storage identity, and weaker static checking',
       'each row can select a new graph').

choice(late_cache_result, cache_and_gate_view,
       'one persistent cache write plus active-scope join',
       'late work can populate cache without appearing in inactive output').
choice(late_cache_result, drop_when_inactive,
       'one active-scope join before cache write',
       'late work is discarded').
choice(late_cache_result, cancellation_only,
       'depends on external cancellation winning every race',
       'late completion remains a correctness hole').

choice(value_storage, relational_rows,
       'declared columns, relation IDs, and ordinary joins',
       'objects and collections normalize into relations').
choice(value_storage, opaque_json,
       'JSON encode/decode and duplicated names',
       'checker loses relational fields and SQL joins').
choice(value_storage, host_closure,
       'runtime registry IDs and non-serializable lifetime',
       'SQLite cannot explain the value').

% Resource model
% --------------

complexity(state_space, 'O(K)',
           'one keyed state row per live scan key, plus separately keyed nested child state').
complexity(queue_space, 'O(E)',
           'queued occurrences for the current transaction; duplicate Log rows remain separate').
complexity(candidate_space, 'O(C)',
           'candidate derivations for the current event; strict reducer requires C = 1').
complexity(side_writes, 'O(W)',
           'Log outputs preserve W occurrences; Set outputs publish their net boundary delta').
complexity(time, 'O(E * StepSQL)',
           'events for one key are sequential; each step runs a fixed prepared SQL statement group').
complexity(parallelism, 'partition by key',
           'different keys can run independently after one ingress order assigns event positions').
complexity(nested, 'O(sum active child keys)',
           'named child relations avoid materializing a cross-product of runtime graph values').

% Exact implementation order and blockers
% ---------------------------------------

implementation_step(1, ordered_event_loop,
                    'finish and grade the in-flight pre lowering against all 13 fixtures and naive/incremental tick-log identity').
implementation_step(2, reducer_signature,
                    'project reducer reads, writes, grade, cardinality, lifetime, and effects from the specialized graph').
implementation_step(3, strict_candidate_check,
                    'prove det statically for the first accepted shapes and transactionally reject zero/many otherwise').
implementation_step(4, named_specialization,
                    'specialize scan(EventRel, StateRel, ReducerRule) to the canonical N-1 rules; emit no function value').
implementation_step(5, golden_use,
                    'write ghcacher and extraction reducers with ordinary rels first, then compare their specialized graphs byte-for-byte').
implementation_step(6, surface,
                    'only after the goldens, parse optional leading semicolon plus Guard |-> Value/Head and Guard |+> EventWrite').

blocker(ordered_event_loop,
        'the committed compiler named edge_body_needs_pre; an uncommitted implementation is in flight in analyze/lower/emitter/runtime',
        '13 pre fixtures compile and naive/incremental logs equal the oracle').
blocker(reducer_cardinality,
        'relplan/5 and plan/6 do not carry det/semidet/multi or derivation counts',
        'zero and overlapping-arm fixtures fail before state mutation; one-arm fixture passes').
blocker(transactional_failure,
        'strict dynamic cardinality checks must cover every event in the tick',
        'event N failure leaves no state or side writes from events 0..N').
blocker(init_contract,
        'the current oracle silently drops an event that precedes same-batch context initialization',
        'missing-init and reversed-order fixtures produce a named refusal/rollback').
blocker(ingress_order,
        'schedule lists are ordered in the oracle; the production contract for concurrently merged host sources is not yet a checked signature',
        'one stable sequence is visible to scan across two sources').
blocker(arm_scope,
        'a sibling-only variable in a body atom currently becomes a fresh wildcard',
        'expansion alpha-renames arm locals and refuses unbound output variables').
blocker(compound_value_encoding,
        'oracle unifies enum/object terms while emitted arrivals and emitted heads use incompatible term encodings',
        'enum reducer fixtures match oracle in emitted runtime without JSON dictionary storage').
blocker(list_values,
        'list fan-out and typed collection lowering are incomplete',
        'deferred from the minimal contract; relation-valued collections cover the goldens').
blocker(clock_signature,
        'grade and effect obligations are implicit in arrows and frontier placement',
        'reducer host effect is refused, |-> is grade 0, and |+> descendants report +1').

% Runnable receipts against the current expander and oracle
% ---------------------------------------------------------

go :-
    receipt_current_match_expands_to_independent_rules,
    receipt_match_has_none_some_multi_rows,
    receipt_equal_rows_dedupe_after_multiple_derivations,
    receipt_scan_preserves_duplicate_occurrences_and_key_partition,
    receipt_same_batch_init_order_is_observable,
    receipt_side_write_descendant_is_delayed,
    receipt_option_is_explicit_data,
    receipt_strict_reducer_cardinality,
    format("8 PASS~n").

receipt_current_match_expands_to_independent_rules :-
    Sugared =
        prog([],
             [ match(source(X),
                     ((left(X) <- X == a) ; (right(X) <+ X == b))) ]),
    expand_match_program_in_context(
        [],
        Sugared,
        prog([],
             [ (left(X) <- source(X), X == a),
               (right(X) <+ source(X), X == b)
             ])),
    format("PASS current match expands each arm to one ordinary rule~n").

receipt_match_has_none_some_multi_rows :-
    match_program(Program),
    run_program(Program, [source(a)], [], Final, _),
    rel_rows(no_value/1, Final, []),
    rel_rows(one_value/1, Final, [one_value(a)]),
    rel_rows(many_value/1, Final, [many_value(left), many_value(right)]),
    format("PASS match result is a 0/1/N relation~n").

receipt_equal_rows_dedupe_after_multiple_derivations :-
    Program =
        prog([],
             [ match(source(X),
                     ((same_value(X) <- true) ; (same_value(X) <- true))) ]),
    run_program(Program, [source(a)], [], Final, _),
    rel_rows(same_value/1, Final, [same_value(a)]),
    format("PASS B-set storage dedupes equal rows after two arm derivations~n").

receipt_scan_preserves_duplicate_occurrences_and_key_partition :-
    scan_program(Program),
    run_program(
        Program,
        [total(a, 0), total(b, 10)],
        [[+add(a, duplicate, 1), +add(b, one, 2),
          +add(a, duplicate, 1)]],
        Final,
        Deltas),
    rel_rows(total/2, Final, [total(a, 2), total(b, 12)]),
    rel_deltas(total/2, Deltas,
               [[-total(a, 0), -total(b, 10),
                 +total(a, 2), +total(b, 12)], []]),
    format("PASS scan preserves duplicate Log occurrences and partitions state by key~n").

receipt_same_batch_init_order_is_observable :-
    switch_scan_program(Program),
    run_program(
        Program,
        [],
        [[+page_event(session, 2),
          +page_change(session, page_a),
          +page_event(session, 3)]],
        Final,
        _),
    % This pins the current oracle fact. The leading event finds no state and
    % is silently absent from the fold. The strict candidate contract above
    % replaces that silence with a named rollback.
    rel_rows(machine/3, Final, [machine(session, page_a, 3)]),
    format("PASS current oracle observes same-batch init/event order~n").

receipt_side_write_descendant_is_delayed :-
    side_write_program(Program),
    run_program(Program, [state(k, 0)], [[+input(k, 1)]], Final, Deltas),
    rel_rows(state/2, Final, [state(k, 1)]),
    rel_rows(side/2, Final, [side(k, 1)]),
    rel_rows(downstream/2, Final, [downstream(k, 1)]),
    rel_deltas(side/2, Deltas, [[+side(k, 1)], [], []]),
    rel_deltas(downstream/2, Deltas, [[], [+downstream(k, 1)], []]),
    format("PASS side event writes now and its descendant fires next tick~n").

receipt_option_is_explicit_data :-
    option_rows([], [none]),
    option_rows([value], [some(value)]),
    catch(option_rows([a, b], _), option_cardinality(multi), true),
    format("PASS Option turns 0/1 into exactly one explicit value without null~n").

receipt_strict_reducer_cardinality :-
    strict_next([next], next),
    catch(strict_next([], _), scan_cardinality(zero), Zero = caught),
    catch(strict_next([a, b], _), scan_cardinality(multi), Multi = caught),
    Zero == caught,
    Multi == caught,
    format("PASS strict scan accepts one derivation and refuses zero/multi~n").

match_program(
    prog([],
         [ match(source(X),
                 ( (no_value(X) <- X == missing)
                 ; (one_value(X) <- X == a)
                 ; (many_value(left) <- X == a)
                 ; (many_value(right) <- X == a)
                 ))
         ])).

scan_program(
    prog([kind(add/3, log), keep(add/3, all), keyed(total/2, [1])],
         [ (total(Key, Next) <+
               add(Key, _EventId, Amount),
               pre(total(Key, Previous)),
               Next := Previous + Amount)
         ])).

switch_scan_program(
    prog([ kind(page_change/2, log), keep(page_change/2, all),
           kind(page_event/2, log), keep(page_event/2, all),
           keyed(machine/3, [1])
         ],
         [ (machine(Owner, Context, 0) <+ page_change(Owner, Context)),
           (machine(Owner, Context, Next) <+
               page_event(Owner, Amount),
               pre(machine(Owner, Context, Previous)),
               Next := Previous + Amount)
         ])).

side_write_program(
    prog([ kind(input/2, log), keep(input/2, all),
           keyed(state/2, [1]),
           kind(side/2, log), keep(side/2, all),
           kind(downstream/2, log), keep(downstream/2, all)
         ],
         [ (state(Key, Next) <+
               input(Key, Amount),
               pre(state(Key, Previous)),
               Next := Previous + Amount),
           (side(Key, Amount) <+ input(Key, Amount)),
           (downstream(Key, Amount) <+ side(Key, Amount))
         ])).

option_rows([], [none]).
option_rows([Value], [some(Value)]).
option_rows([_, _ | _], _) :-
    throw(option_cardinality(multi)).

strict_next([Next], Next).
strict_next([], _) :-
    throw(scan_cardinality(zero)).
strict_next([_, _ | _], _) :-
    throw(scan_cardinality(multi)).
