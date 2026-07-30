% Select + scan + switch-cache lab.
%
% Consult:
%   swipl -q -l plans/2026-07-30-select-scan-cache-lab.pl
%
% Validate the record:
%   swipl -q -l plans/2026-07-30-select-scan-cache-lab.pl -g go -g halt
%
% Run executable receipts:
%   swipl -q -l v6/prolog/labs/select_scan_cache/0_receipts.pl -g go -g halt

:- module(select_scan_cache_lab,
          [go/0,
           goal/2,
           formalism/3,
           lowering/3,
           primitive/2,
           table_shape/3,
           clock/3,
           golden_fixture/3,
           choice_point/2,
           route/4]).

:- use_module(library(aggregate), [aggregate_all/3]).

:- discontiguous choice_point/2.
:- discontiguous route/4.
:- discontiguous provisional/2.

% Context and goal

goal(one_formalism,
     'merged Log events -> ordered select -> match(previous,event) -> keyed scan state -> boundary delta -> delayed listener or host event').
goal(actual_world,
     'reuse current prog/2, <-, <+, latest, pre, keyed replacement, finalize, host demand/response, compiler plan, and SQLite lowering').
goal(generic_cache,
     'prove makeSwitchMapCache with compile-time named relation and rule arguments; erase it before runtime').
goal(reversibility,
     'add no production syntax, runtime function value, dynamic table, new tick phase, or reentrant dispatch').

evidence(reference_oracle,
         'v6/prolog/conformance/engine.pl',
         'outside arrivals are an ordered list; pre reads evolving state; boundary exposes only start/end B-set delta; carry runs next tick').
evidence(existing_switch,
         'v6/prolog/conformance/fixtures/scopes.pl',
         'switchMap is keyed replacement plus ordinary derived demand/output relations').
evidence(existing_higher_order_lab,
         'v6/prolog/labs/higher_order_scan/0_receipts.pl',
         'named relation/rule arguments specialize into prog/2 and erase before SQL').
evidence(scan_runtime_lane,
         'v6/tsv2/tests/orderedPre.test.ts',
         'ordered pre implementation is concurrently proving emitted runtime parity; this lab edits none of it').
evidence(match_surface,
         'v6/prolog/compile/parse_dl.pl',
         'Guard |-> Head parses as a level arm; Guard |+> Head parses as an edge arm').

% One formalism

formalism(ingress,
          'MergedEvent<Key,Sequence,Variant>',
          'one Log relation receives an ordered outside-arrival list; duplicates remain occurrences').
formalism(reduce,
          'State[T,i+1] = Match(State[T,i], Event[T,i])',
          'pre reads the evolving keyed state after earlier event positions in the same tick').
formalism(commit,
          'delta(State[T,0], State[T,end])',
          'intermediate reducer states are private; one net keyed boundary delta is observable').
formalism(listener,
          'CommittedDelta[T] -> EffectEvent[T+1]',
          'ordinary <+ carry supplies the delayed listener queue').
formalism(host,
          'Demand[T] -> Response[T+n], n >= 1',
          'host completion re-enters as an outside arrival; stale output is rejected by current scope membership').

surface('|->',
        'Guard |-> Value',
        'pure match result; in scan it contributes a candidate next state').
surface('|+>',
        'Guard |+> Event',
        'ordinary side write; descendants run from a later carry').
surface(dirty_escape,
        reserved,
        'a future explicit immediate/reentrant form may be checker-gated; this arc adds none').

% Tier N call. The runnable lab uses these named arguments.

generic_call(
    make_switch_map_cache,
    [outer(route_change/2),
     key_rule(route_key/2),
     scope(open_scope/2),
     response(route_response/2),
     decode_rule(decode_value/2),
     cache(route_cache/2),
     demand(route_demand/1),
     output(route_output/3),
     finalized(route_finalized/2),
     effect(route_effect/1)]).

% Tier N-1 result.

specialized_rule(scope,
                 'open_scope(Owner, Key) <+ route_change(Owner, Input), latest(route_key(Input, Key))').
specialized_rule(cache_fill,
                 'route_cache(Key, Value) <+ route_response(Key, Wire), latest(decode_value(Wire, Value))').
specialized_rule(cache_miss,
                 'route_demand(Key) <- open_scope(_, Key), not(route_cache(Key, _))').
specialized_rule(output_gate,
                 'route_output(Owner, Key, Value) <- open_scope(Owner, Key), route_cache(Key, Value)').
specialized_rule(finalize,
                 'route_finalized(Owner, Key) <+ finalize(open_scope(Owner, Key))').
specialized_rule(effect,
                 'route_effect(Key) <+ route_demand(Key)').

% Which work is already first-order and which work needs specialization.

lowering(scope_replacement, ordinary,
         'keyed relation write plus current <+ semantics').
lowering(cache_fill, ordinary,
         'response occurrence, sampled decoder relation, and keyed write').
lowering(hit_or_miss, ordinary,
         'join or antijoin against cache membership').
lowering(stale_output_rejection, ordinary,
         'active-scope join gates output').
lowering(finalize, ordinary,
         'negative boundary delta subscribed through finalize/1').
lowering(effect_delay, ordinary,
         'edge-written carry supplies grade +1').
lowering(scan_arithmetic, ordinary,
         'pre(state), integer arithmetic, keyed replacement, and ordered occurrences').
lowering(match, ordinary,
         'arms expand to ordinary <- or <+ rules').
lowering(named_argument_resolution, specialization,
         'resolve relation/rule symbols and validate their canonical signatures').
lowering(helper_hygiene, specialization,
         'mint collision-free call-site helper relation names when the caller did not provide names').
lowering(call_erasure, specialization,
         'replace makeSwitchMapCache or scan call with concrete rules before checked IR and SQL').
lowering(reuse, specialization,
         'reuse an identical specialized graph by a stable specialization identity; storage remains caller-named').

% Minimum kernel

primitive(log_occurrence,
          'ordered outside arrivals with duplicate preservation').
primitive(keyed_set,
          'one current state or scope row per declared key').
primitive(level_rule,
          'pure grade-0 join, antijoin, guard, and match result').
primitive(edge_rule,
          'state write with next-tick downstream carry').
primitive(pre,
          'evolving keyed state before this event position').
primitive(latest,
          'sample a relation without making it another trigger source').
primitive(finalize,
          'observe a committed negative membership delta').
primitive(integer_arithmetic,
          'retry, page, sequence, and accumulated count transitions').
primitive(named_compile_time_argument,
          'relation/rule symbol plus canonical schema, key, clock, cardinality, lifetime, and effects').
primitive(host_demand_response,
          'committed demand membership and later response arrival').

excluded(runtime_function_value).
excluded(relation_value_in_sqlite_column).
excluded(dynamic_table_creation).
excluded(implicit_order_from_sql_rows).
excluded(same_tick_effect_reentry).
excluded(new_tick_phase).

% Storage and SQL

table_shape(route_change, log,
            'outer occurrences; retention declared').
table_shape(route_response, log,
            'async host completions; retention declared').
table_shape(open_scope, keyed([owner]),
            'PRIMARY KEY(owner) WITHOUT ROWID; replacement selects active cache key').
table_shape(route_cache, keyed([cache_key]),
            'PRIMARY KEY(cache_key) WITHOUT ROWID; durable fills survive switching').
table_shape(route_demand, level,
            'materialized support table from scope antijoin cache').
table_shape(route_output, level,
            'materialized support table from active scope join cache').
table_shape(route_finalized, log,
            'delayed scope-departure occurrences').
table_shape(route_effect, log,
            'post-commit listener side writes').
table_shape(machine, keyed([scan_key]),
            'one Redux-style scan register per key').
table_shape(select_event, log,
            'merged ordered select queue; variant is relational/enum data').

sql_shape(scope_write,
          'INSERT open_scope ... ON CONFLICT(owner) DO UPDATE').
sql_shape(cache_write,
          'INSERT route_cache ... ON CONFLICT(cache_key) DO UPDATE').
sql_shape(miss,
          'SELECT scope.key WHERE NOT EXISTS cache(key)').
sql_shape(output,
          'SELECT owner,key,value FROM scope JOIN cache USING(key)').
sql_shape(scan_step,
          'read __pre_machine, project arithmetic, validate one keyed result, write machine').
sql_shape(specialization,
          'fixed DDL and statements; no makeSwitchMapCache, scan, closure, or relation value survives').

complexity(state_memory, 'O(active scan keys + active switch owners)').
complexity(cache_memory, 'O(distinct retained cache keys)').
complexity(queue_memory, 'O(current event batch + next carry)').
complexity(scan_work, 'O(events times applicable match arms); indexed state lookup by key').
complexity(cache_read, 'indexed scope/cache join or antijoin').
complexity(specialization, 'O(call sites) before reuse; zero runtime dispatch').

% Clock signature

clock(reducer_read, grade(0),
      'State[T,i] and Event[T,i]').
clock(reducer_write, grade(0),
      'State[T,i+1], private until boundary').
clock(boundary_state, boundary,
      'State[T+1,0] and its net signed delta').
clock(listener_event, grade(1),
      'committed demand/state delta produces later edge work').
clock(async_response, at_least(1),
      'host completion cannot re-enter the invoking reduction').
clock(finalize_event, grade(1),
      'committed scope departure becomes a later occurrence').

% Executed receipts

receipt(cache_hit,
        pass,
        'warm cache yields output; demand and effect remain empty').
receipt(cache_miss,
        pass,
        'miss commits demand; effect arrives one tick later').
receipt(async_response,
        pass,
        'response scheduled later fills cache and retracts demand').
receipt(switch,
        pass,
        'new outer value replaces keyed active scope').
receipt(late_fill,
        pass,
        'response for retired scope still populates durable cache').
receipt(stale_output_rejection,
        pass,
        'late fill cannot join inactive scope').
receipt(multiple_keys,
        pass,
        'two owners retain independent active scopes and outputs').
receipt(finalize,
        pass,
        'replacement produces one delayed finalized occurrence').
receipt(retry_pagination_arithmetic,
        pass,
        'page, retry, success, and accumulated count fold through ordered pre').
receipt(source_order,
        pass,
        'retry then success differs from success then retry in one merged Log').
receipt(rollback,
        pass,
        'two reducer rows for one key and occurrence throw keyed_conflict before application').
receipt(side_write,
        pass,
        '|-> parses to pure arm and |+> parses to delayed edge arm').
receipt(sql_erasure,
        pass,
        'specialized graph reaches program_plan, lower_program, and TypeScript emission without generic terms').

% Golden fixture promotion

golden_fixture(select_scan_reducer,
               'merge click, response, timer, and reset variants into one ordered Log; match previous/event; prove one net state delta',
               'oracle and emitted naive/incremental tick logs byte-identical').
golden_fixture(make_switch_map_cache,
               'warm hit, miss, switch, late fill, stale gate, multiple owners, finalize, effect demand, async response',
               'oracle and emitted SQLite final state plus tick log identical').
golden_fixture(ghcacher_switch_cache,
               'poll -> keyed demand -> host response -> cache -> output; retry and ETag state use scan arithmetic',
               'replace handwritten scope/cache graph with specialization and compare checked graph, SQL, and golden tick log').
golden_fixture(extract_switch_scan,
               'file revision switch -> extraction demand -> batched facts; stale revision fills cache without publishing inactive facts',
               'edit retraction and later result obey scope key and clock signature').
golden_fixture(rollback,
               'ambiguous reducer or decoder result during one tick',
               'whole transaction restores state, queue, support tables, and effect demand').

% Three bounded production choices. Each has exactly three priced routes.

choice_point(global_source_order,
             'how multiple host/source adapters enter one deterministic select queue').
route(global_source_order, single_ingress_serializer,
      'reuse the existing ordered outside-arrival list; one append authority per engine',
      'selected by this lab; adapters normalize before scan').
route(global_source_order, sequence_sort_with_watermark,
      'buffer by source sequence until a watermark proves no earlier event remains',
      'adds buffer state, watermark protocol, and latency').
route(global_source_order, refuse_multi_source_scan,
      'allow scan over one Log relation only',
      'authors build or host the merge explicitly').
provisional(global_source_order, single_ingress_serializer).

choice_point(reducer_cardinality,
             'where exactly-one next state is enforced').
route(reducer_cardinality, runtime_candidate_count,
      'count candidates per event/key before keyed write',
      'scrappy path; existing keyed_conflict proves the many-row failure, zero-row check remains').
route(reducer_cardinality, static_det_proof,
      'prove match coverage and arm disjointness from types and guards',
      'no runtime count; checker work grows with guard theory').
route(reducer_cardinality, explicit_priority,
      'first successful arm wins',
      'changes match from relational union to ordered choice inside scan').
provisional(reducer_cardinality, runtime_candidate_count).

choice_point(specialization_reuse,
             'identity used to reuse equal generic expansions').
route(specialization_reuse, call_site_identity,
      'one expansion per call site with caller-named state',
      'works now; duplicate stateless SQL may remain').
route(specialization_reuse, definition_closure_hash,
      'hash algorithm version plus canonical signatures and relation-definition closure',
      'deduplicates equal graphs; recursive SCC hashing and migration rules required').
route(specialization_reuse, authored_instance_name,
      'author explicitly names a shared specialized instance',
      'zero hash machinery; more surface and manual coordination').
provisional(specialization_reuse, call_site_identity).

next_step(1, promote_cache_fixture,
          'copy the generic cache graph into a compiler fixture and require oracle/emitted parity').
next_step(2, finish_ordered_scan_runtime,
          'land the concurrent ordered-pre implementation and rerun retry/pagination in both modes').
next_step(3, add_cardinality_zero_guard,
          'abort a scan event that produces no next state before committing any write').
next_step(4, ghcacher_replace,
          'specialize the built-in over ghcacher named relations and diff against the handwritten golden').
next_step(5, extract_replace,
          'specialize the same built-in over file/revision extraction relations and diff goldens').
next_step(6, decide_surface,
          'select syntax only after both golden replacements reuse the same canonical signature').

go :-
    aggregate_all(count, receipt(_, pass, _), ReceiptCount),
    ReceiptCount =:= 13,
    aggregate_all(count, choice_point(_, _), ChoiceCount),
    ChoiceCount =:= 3,
    forall(
        choice_point(Choice, _),
        (aggregate_all(count, route(Choice, _, _, _), RouteCount),
         RouteCount =:= 3)),
    forall(provisional(Choice, Route),
           route(Choice, Route, _, _)),
    format('13 receipt records, 3 choice points, 3 routes each: PASS~n').
