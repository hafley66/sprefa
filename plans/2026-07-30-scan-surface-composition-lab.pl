% Scan surface composition lab over the current V6 implementation.
%
% Consult:
%   swipl -q -l plans/2026-07-30-scan-surface-composition-lab.pl
%
% Validate this record:
%   swipl -q -l plans/2026-07-30-scan-surface-composition-lab.pl \
%     -g go -g halt
%
% Run executable receipts:
%   swipl -q -l v6/prolog/labs/scan_surface_composition/0_receipts.pl \
%     -g go -g halt

:- module(scan_surface_composition_lab,
          [ go/0,
            goal/2,
            locked_model/2,
            relation_signature/3,
            state_identity/2,
            definition_lifetime/3,
            instance_timeline/3,
            storage/3,
            lowering/3,
            edge_case/4,
            finding/3,
            surface_card/4,
            card_lowering/3,
            receipt/3
          ]).

:- use_module(library(aggregate), [aggregate_all/3]).

:- discontiguous surface_card/4.
:- discontiguous card_lowering/3.

goal(existing_world,
     'test scan composition through the current parser-independent prog/2 IR, match expander, oracle, checker, SQL lowerer, and runtime rules').
goal(shared_definition,
     'one reducer definition serves every demand instance and subscriber without cloning work per row or subscriber').
goal(nestable_result,
     'a scan exposes an ordinary named view relation that any later match can consume').
goal(decision_boundary,
     'price surface spellings without selecting syntax or changing production semantics').

locked_model(type_system,
             'relation declarations, columns, keys, rule bodies, and clock grades use the one current checker').
locked_model(storage,
             'every demand, subscriber, event, state cell, view, cancellation, and nested match result is an ordinary relation row').
locked_model(excluded,
             'no relation value, scan value, special scan type, dynamic table, JSON storage, second signature system, or runtime interpretation').
locked_model(surface,
             'all three cards are unselected; the executable lab parses none of them and edits no production file').

% Relation declarations are the type signatures.

relation_signature(demand,
                   'scan_demand(Instance:text, Owner:text, Key:text, Init:int)',
                   'Set keyed by (Instance,Owner,Key)').
relation_signature(subscriber,
                   'scan_subscriber(Subscriber:text, Instance:text, Owner:text, Key:text)',
                   'Set keyed by Subscriber in the receipt').
relation_signature(event,
                   'scan_event(Instance:text, Owner:text, Key:text, EventId:text, Kind:text, Amount:int)',
                   'Log keep(all); its ordered occurrences drive the fold').
relation_signature(state,
                   'scan_cell(Instance:text, Owner:text, Key:text, Value:int)',
                   'Set keyed by (Instance,Owner,Key)').
relation_signature(view,
                   'scan_view(Subscriber:text, Instance:text, Owner:text, Key:text, Value:int)',
                   'level relation joining subscriber, live demand, and state').
relation_signature(cancel,
                   'scan_cancel(Instance:text, Owner:text, Key:text)',
                   'Log keep(all), emitted from finalize(scan_demand)').
relation_signature(observed,
                   'scan_observed(Subscriber:text, Instance:text, Owner:text, Key:text, Value:int)',
                   'Log keep(all), used only to measure the listener clock').

state_identity(key,
               '(DefinitionName, Instance, Owner, Key); DefinitionName is compile-time relation ownership and the stored key is (Instance,Owner,Key)').
state_identity(subscriber,
               'Subscriber is outside the state key, so subscriber count cannot multiply reducer writes').
state_identity(demand,
               'one keyed demand row owns init and liveness for one state identity').

% Definition lifetime and instance lifetime are separate.

definition_lifetime(compile,
                    'one source definition',
                    'expands once into 10 ordinary rules regardless of demand-row count').
definition_lifetime(runtime,
                    'zero to N demand rows',
                    'each demand key selects one independent state cell under the shared rules').
definition_lifetime(subscriber,
                    'zero to N subscriber rows per demand',
                    'each row adds one view result; none appears in reducer bodies').

instance_timeline(demand_add,
                  'tick T',
                  'the keyed demand arrival triggers an init write; a later event occurrence in the same ordered batch reads that init through evolving pre').
instance_timeline(event,
                  'tick T occurrence i',
                  'latest(demand) gates the event, pre(state) reads the preceding accepted state, and one arm proposes the next keyed row').
instance_timeline(boundary,
                  'end of T',
                  'state and the subscriber view publish their net old-to-new deltas; intermediate fold rows remain hidden').
instance_timeline(listener,
                  'tick T+1',
                  'ordinary edge subscribers receive the state-derived view addition as carry').
instance_timeline(subscriber_remove,
                  'tick T',
                  'that subscriber view retracts; demand and state remain').
instance_timeline(demand_remove,
                  'tick T then T+1',
                  'all live views retract at T; finalize(demand) emits cancellation at T+1; later events fail latest(demand)').
instance_timeline(redemand,
                  'later tick',
                  'the demand addition writes its init over the retained inactive cell; same-batch event order then follows ordinary occurrence order').

% Storage, read, write, and uniqueness sequence.

storage(persistent,
        '9 ordinary relation tables in the full receipt',
        'demand, subscriber, event, cell, view, cancel, observed, positive, and nonpositive').
storage(temporary,
        '32 current compiler support tables in the full receipt',
        'includes one __pre_scan_cell table keyed by (instance,owner,key)').
storage(reducer_read,
        'latest(scan_demand) plus pre(scan_cell)',
        'one indexed liveness sample and one keyed evolving-state sample per occurrence').
storage(reducer_write,
        'scan_cell keyed replace',
        'one accepted row changes the cell; equal row is a no-op').
storage(subscriber_read,
        'scan_subscriber join scan_demand join scan_cell',
        'subscriber fanout occurs after reduction as an ordinary level join').
storage(teardown,
        'inactive scan_cell remains stored',
        'current edge heads have no delete operation; re-demand deterministically overwrites it with init').
storage(rule_budget,
        '10 expanded rules for 1 or 1000 demand rows',
        'runtime row count changes data work and result rows, not generated SQL/rule count').

% Exact lab-only definition expansion.

lowering(definition, step(1),
         'declare demand and subscriber keys, event/cancel/observed Log retention, and state key with current declarations').
lowering(definition, step(2),
         'emit State(Identity,Init) <+ Demand(Identity,Init)').
lowering(definition, step(3),
         'expand each reducer arm into State(Identity,Next) <+ Event(Identity,...),Guard,latest(Demand),pre(State),Expression').
lowering(definition, step(4),
         'emit View(Subscriber,Identity,Value) <- Subscriber(...),Demand(...),State(...)').
lowering(definition, step(5),
         'emit Cancel(Identity) <+ finalize(Demand(Identity,_))').
lowering(definition, step(6),
         'erase downstream match View(...) arms into ordinary named level rules').
lowering(definition, step(7),
         'the checked IR contains 10 ordinary <- or <+ rules and no scan_def or match term').

lowering(add_arm,
         source,
         'Kind == add, latest(Demand), pre(State(Previous)), Next := Previous + Amount |+> State(Next)').
lowering(add_arm,
         ir,
         'State(Next) <+ Event(...,Kind,Amount), Kind == add, latest(Demand), pre(State(Previous)), Next := Previous + Amount').
lowering(nested_match,
         source,
         'match scan_view(...) (; Value > 0 |-> scan_positive(...) ; Value =< 0 |-> scan_nonpositive(...))').
lowering(nested_match,
         ir,
         'two ordinary level rules with scan_view as the positive body atom').

edge_case(multiple_demands,
          runtime,
          'alpha/owner/key and beta/owner/key reach 5 and 17 through the same reducer rules',
          pass).
edge_case(multiple_subscribers,
          runtime,
          'two view rows observe state 2 while the state changes from 0 to 2 once',
          pass).
edge_case(subscriber_remove,
          runtime,
          'one view retracts and the other remains; no state write occurs',
          pass).
edge_case(demand_remove,
          runtime,
          'remaining views retract, cancellation follows one tick later, and inactive state remains',
          pass).
edge_case(event_without_demand,
          runtime,
          'latest(demand) fails and the event makes no state write',
          pass).
edge_case(init,
          runtime,
          'demand addition writes init through the same ordered edge mechanism',
          pass).
edge_case(reset,
          runtime,
          'reset arm samples demand init and replaces state',
          pass).
edge_case(redemand_then_event,
          runtime,
          'demand listed before event initializes first and the event reads init; reverse order ends at init when an inactive cell existed',
          current_ordering).
edge_case(zero_candidate,
          reducer,
          'no matching arm makes no write and raises no error',
          current_semidet).
edge_case(one_candidate,
          reducer,
          'one candidate writes one keyed state row',
          pass).
edge_case(differing_many,
          reducer,
          'different rows for the same state key throw keyed_conflict before application',
          pass).
edge_case(equal_many,
          reducer,
          'equal candidates dedupe before application',
          current_dedupe).
edge_case(nested_match,
          composition,
          'downstream match consumes the named scan_view relation and stays in the level fixpoint',
          pass).
edge_case(direct_anonymous_nesting,
          parser,
          'current parser has no scan form and accepts match only as a top-level statement',
          refused).
edge_case(physical_state_teardown,
          runtime,
          'current edge heads cannot delete the inactive keyed cell',
          gap).
edge_case(conflicting_init_same_identity,
          storage,
          'keyed demand replacement picks one current init and its addition resets the state',
          current).

finding(shared_demand,
        supported,
        'multiple demand rows are data partitions of one generated reducer graph').
finding(shared_subscription,
        supported,
        'multiple subscribers join one state result and do not cause reducer occurrences').
finding(nesting,
        supported_named,
        'an ordinary named view is the composition boundary for later match blocks').
finding(anonymous_nesting,
        requires_surface_only,
        'a compiler may generate a stable ordinary view name and source map; runtime semantics stay unchanged').
finding(cancellation,
        split_clock,
        'view retraction occurs at T and finalize cancellation is observable at T+1').
finding(cell_cleanup,
        unresolved,
        'physical deletion needs a separate existing-lifecycle lowering decision; the lab retains and resets the cell').
finding(exact_one,
        unresolved,
        'current semidet behavior remains zero silent, differing N conflict, equal N dedupe').
finding(bool_lane_regression,
        closed_external,
        'the initial SQL-count run found literal_witness/1 binding variable witnesses to bool_lit(true); the phase-5 lane added the required nonvar guard and the real lowerer receipt then passed').

% Three unselected cards. Each lowers to the same ordinary graph. Surface text
% is data here and is never sent to the production parser.

surface_card(
    ordinary_named_rules,
    baseline,
    'write demand/init rules and `match scan_event(...)` plus `match scan_view(...)` as separate top-level blocks',
    'zero syntax; full lifecycle is verbose; every relation and clock edge stays visible').
card_lowering(
    ordinary_named_rules,
    exact,
    'the source already is the 10-rule expansion used by 0_receipts.pl').
card_lowering(
    ordinary_named_rules,
    nesting,
    'name scan_view and use it as the source of the next top-level match').

surface_card(
    named_scan_block,
    candidate,
    'scan counter_scan(demand scan_demand(...), subscribe scan_subscriber(...), event scan_event(...), state scan_cell(...), view scan_view(...), cancel scan_cancel(...)) ( ; Guards |+> scan_cell(...) )',
    'adds one declaration/block grammar; names remain compile-time relation slots; compiler emits the exact 10-rule graph and validates that arms write the declared keyed state').
card_lowering(
    named_scan_block,
    exact,
    'header emits declarations, init/view/cancel rules; each arm becomes State <+ Event,Guards; downstream match still names scan_view').
card_lowering(
    named_scan_block,
    ramifications,
    'needs diagnostics for mismatched keys, arms writing another head, missing init, and name collisions; adds no runtime object or table category').

surface_card(
    nested_scan_block,
    candidate,
    'match scan counter_scan(...) ( ; Guards |+> scan_cell(...) ) as scan_view(...) ( ; ViewGuards |-> Head )',
    'adds inline grammar and a compiler-owned stable ordinary view name; query/debug output needs source mapping for that generated name').
card_lowering(
    nested_scan_block,
    exact,
    'lower the inner scan exactly as named_scan_block, mint one stable __scan_<definition_hash>_view relation, then lower the outer match over that relation').
card_lowering(
    nested_scan_block,
    ramifications,
    'rule/table counts match the named expansion; stable naming and collision rules become compiler obligations; no anonymous relation value survives lowering').

receipt(erasure, pass,
        'lab definition erases to 10 ordinary rules and existing declarations').
receipt(instances, pass,
        'one reducer definition partitions two demand identities').
receipt(subscribers, pass,
        'two subscribers observe one reduction').
receipt(lifecycle, pass,
        'subscriber and demand removal, cancellation, gating, and inactive-cell retention are measured').
receipt(init_reset, pass,
        'init, reset, re-demand, and same-batch occurrence order are measured').
receipt(cardinality, pass,
        'zero, one, differing N, and equal N behaviors are measured').
receipt(nesting, pass,
        'named view followed by match is measured').
receipt(clocks, pass,
        'T state/view boundary and T+1 edge observation are measured').
receipt(storage, pass,
        'real SQL lowerer emits 9 persistent tables, 32 TEMP tables, and 1 __pre_scan_cell').
receipt(rule_budget, pass,
        'expanded rule count stays 10 at 1 and 1000 demand rows').

go :-
    aggregate_all(count, surface_card(_, _, _, _), CardCount),
    CardCount =< 3,
    forall(
        surface_card(Card, _, _, _),
        ( aggregate_all(count, card_lowering(Card, _, _), LoweringCount),
          LoweringCount >= 2
        )),
    aggregate_all(count, receipt(_, _, _), 10),
    aggregate_all(count, receipt(_, pass, _), Passing),
    Passing =:= 10,
    format("10 receipt records: 10 pass; ~d unselected surface cards~n",
           [CardCount]).
