% Scan and match reconciliation against the current V6 tree.
%
% Consult:
%   swipl -q -l plans/2026-07-30-scan-match-reconciliation.pl
%
% Validate this record:
%   swipl -q -l plans/2026-07-30-scan-match-reconciliation.pl \
%     -g go -g halt
%
% Run executable receipts:
%   swipl -q -l v6/prolog/labs/scan_match_reconciliation/0_receipts.pl \
%     -g go -g halt

:- module(scan_match_reconciliation,
          [ go/0,
            goal/2,
            locked_model/2,
            current_semantics/3,
            lowering/3,
            timeline/3,
            storage/3,
            edge_case/4,
            gap/3,
            existing_lab/1,
            lab_status/3,
            open_question/2,
            option/4,
            receipt/3
          ]).

:- use_module(library(aggregate), [aggregate_all/3]).

:- discontiguous open_question/2.
:- discontiguous option/4.

goal(existing_world,
     'reconcile scan and match from the current parser, expander, oracle, checker, SQL lowerer, emitter, runtime, and tests').
goal(one_model,
     'keep every entity, intermediate, state cell, enum case, and reference as an ordinary declared or compiler-generated rel').
goal(no_orphans,
     'classify every prior scan or match lab as implemented, canonical_plan, closed, or superseded').

locked_model(type_system,
             'one checker over rel columns, keys, rule bodies, clocks, and storage').
locked_model(storage,
             'one SQL table graph with compiler-owned normalization for nested rel references').
locked_model(values,
             'scalar columns and relation identities only; nested data becomes rows and reference edges').
locked_model(excluded,
             'no relation-like intermediate type, runtime relation value, parallel signature type system, JSON dictionary, nested JSON storage, or dynamic table').
locked_model(surface,
             'this reconciliation adds no syntax and makes no scan spelling decision').

% Current type signatures are ordinary relation schemas. Rule direction and
% body use derive the rest.

current_semantics(match,
                  'match Source (Guards |-> Head ; Guards |+> Head)',
                  'top-level sugar; each arm expands to one ordinary Head <- Source,Guards or Head <+ Source,Guards rule').
current_semantics(match_result,
                  'Rel<Columns>',
                  'zero, one, or many rows; equal rows collapse in B-set storage').
current_semantics(scan_n_minus_one,
                  'Event Log + keyed State Set + State <+ Event,pre(State),Body',
                  'ordered occurrences update the evolving __pre_State table and publish only the boundary state delta').
current_semantics(nested_match,
                  'MatchA -> named ordinary rel -> MatchB',
                  'two top-level blocks compose through a normal rel table in the level fixpoint').
current_semantics(scan_with_match,
                  'one match block over Event with State edge heads',
                  'arm expansion produces guarded ordered edge rules; no anonymous result or nested block is needed').

lowering(match, step(1),
         'parse one top-level match/2 term with left-to-right arm spelling').
lowering(match, step(2),
         '0_match_expand.pl emits one ordinary rule per arm and erases match before checked IR').
lowering(match, step(3),
         'ordinary level or edge lowering emits fixed SQL for the arm heads').
lowering(nested_match, step(1),
         'name an intermediate rel and make it the head of the first match').
lowering(nested_match, step(2),
         'make the intermediate rel the source of the second match').
lowering(nested_match, step(3),
         'level closure orders the generated ordinary rule groups by dependencies').
lowering(scan, step(1),
         'outside Log arrivals retain input order and duplicates').
lowering(scan, step(2),
         'each ordered edge arm reads the current __pre_State row').
lowering(scan, step(3),
         'the keyed write updates State and mirrors the accepted row into __pre_State').
lowering(scan, step(4),
         'the runtime computes one tick-start to tick-end boundary delta').
lowering(scan, step(5),
         'edge listeners receive boundary additions through the next carry tick').

timeline(match_level,
         'T level closure',
         'all named match stages settle in the same level fixpoint; intermediate tables are queryable').
timeline(scan_event,
         'T occurrence i',
         'read State[T,i], evaluate every applicable generated arm, then apply accepted keyed write').
timeline(scan_next,
         'T occurrence i+1',
         'read the row written by occurrence i through __pre_State').
timeline(scan_boundary,
         'end of T',
         'publish only -State[T,0] and +State[T,end] when changed').
timeline(scan_listener,
         'T+1 carry',
         'ordinary <+ listeners react to the committed state addition').

storage(nested_match,
        '4 persistent rel tables and 15 TEMP support tables in the executable receipt',
        'input, stage, positive, and nonpositive are ordinary queryable rels; no nested value column exists').
storage(scan_match,
        '2 persistent rel tables and 7 TEMP support tables in the executable receipt',
        'select_event and machine are authored; __pre_machine is the only scan-specific TEMP table').
storage(scan_specialization,
        '0 generated persistent tables when event and state rels are already named',
        'a compiler helper may generate ordinary rules; it must not generate a new value category').
storage(nested_state,
        'one ordinary keyed child rel',
        'parent and child identities occupy key columns and normal reference edges').

edge_case(match_no_arm,
          general_match,
          'zero result rows',
          current).
edge_case(match_one_arm,
          general_match,
          'one result row',
          current).
edge_case(match_many_arms,
          general_match,
          'union of all rows; equal rows collapse after derivation',
          current).
edge_case(direct_nested_block,
          parser,
          'dl_parse_error(statement, Position)',
          current_refusal).
edge_case(named_nested_match,
          compiler,
          'two top-level matches joined by one ordinary rel',
          current).
edge_case(scan_zero_candidate,
          scan,
          'event makes no state write and raises no error',
          gap).
edge_case(scan_one_candidate,
          scan,
          'one keyed state write',
          current).
edge_case(scan_many_different,
          scan,
          'keyed_conflict(StateRel, Key, Rows)',
          current).
edge_case(scan_many_equal,
          scan,
          'equal writes dedupe and one row is applied',
          gap).
edge_case(scan_missing_init,
          scan,
          'pre finds no row; the event makes no state write',
          gap).
edge_case(scan_seed_then_event,
          scan,
          'event sees the authored seed write earlier in the same ordered batch',
          current).
edge_case(scan_event_then_seed,
          scan,
          'event is silent; later seed becomes the state',
          current).
edge_case(scan_duplicate_log_events,
          scan,
          'each outside Log occurrence occupies one queue position',
          current).
edge_case(scan_set_duplicate,
          scan,
          'Set membership contributes no second occurrence',
          current).
edge_case(scan_cross_source_order,
          scan,
          'outside arrivals retain the supplied array order; concurrent host ordering is owned by ingress',
          current_boundary).
edge_case(scan_side_write,
          clock,
          'write occurs during T; descendants receive carry at T+1',
          current).
edge_case(scan_transaction_error,
          runtime,
          'current oracle aborts the run on keyed_conflict; whole emitted-tick rollback remains a separate transaction task',
          gap).

gap(exact_one_zero,
    no_named_error,
    'the current edge-rule model is semidet and silently accepts zero next-state rows').
gap(exact_one_equal_many,
    no_derivation_count_before_dedup,
    'two equal candidates are indistinguishable after write deduplication').
gap(scan_surface,
    no_scan_construct,
    'scan exists only as ordinary Event, State, pre, and edge rules').
gap(stale_higher_order_receipt,
    false_positive_catch_binding,
    'scan_compiler_gap_receipt leaves ScanError unbound when program_plan succeeds, then unifies it with edge_body_needs_pre').
gap(direct_match_nesting,
    'dl_parse_error(statement, Position)',
    'the parser accepts match only as a top-level statement').
gap(compound_arrival_match,
    'edge_body_needs_json_destructure(Body)',
    'compound arrival encoding still blocks decode/json_each in edge arms').
gap(transactional_failure,
    emitted_tick_transaction,
    'a future strict candidate check would need rollback before exposing any event writes').

existing_lab(match_frontier).
existing_lab(match_block).
existing_lab(higher_order_rel_scan).
existing_lab(scan_match_value).
existing_lab(generic_scan_instantiation).
existing_lab(select_scan_cache).
existing_lab(ordered_pre).

lab_status(match_frontier, closed,
           'the 2026-07-28 lab completed its frontier and lifecycle contradiction census; later match syntax replaced its open surface slots').
lab_status(match_block, implemented,
           '0_match_expand.pl, parse_dl.pl, print_dl.pl, enum coverage, SQL identity tests, and fixtures exist').
lab_status(higher_order_rel_scan, superseded,
           'its catch leaves the error variable unbound on success, then unifies it with edge_body_needs_pre; ordered-pre lowering now succeeds').
lab_status(scan_match_value, superseded,
           'its current 0/1/N receipts remain evidence; RuleRef, scan signature, and future surface proposals are excluded').
lab_status(generic_scan_instantiation, closed,
           'the prototype measured 0 generated persistent tables and 1 TEMP pre table; scan_signature and relation-role metadata remain lab-only').
lab_status(select_scan_cache, canonical_plan,
           'its ordinary-rel switch cache, stale-result gate, and delayed effect graph remain the golden algorithm plan').
lab_status(ordered_pre, implemented,
           'analyzer, lowerer, emitter, runtime staging, and focused tests exist in the current tree').

% Open questions remain inside existing relation and rule mechanics. Each has
% at most three options. No option introduces a relation-like value type.

open_question(zero_candidate,
              'what should an event with no matching next-state rule mean when a program is intended as a total scan').
option(zero_candidate, current_semidet,
       'keep current behavior',
       'zero writes; no checker or runtime work').
option(zero_candidate, authored_keep_arm,
       'author an explicit catch-all State(Key,Previous) edge rule',
       'totality is visible but overlap can keyed_conflict').
option(zero_candidate, future_checked_totality,
       'add a checker/runtime obligation over ordinary generated edge arms',
       'requires a named failure and whole-tick rollback; no new stored type').

open_question(equal_many,
              'whether total scan must distinguish two equal derivations from one derivation').
option(equal_many, current_write_identity,
       'count distinct keyed writes',
       'equal candidates dedupe and succeed').
option(equal_many, static_disjointness,
       'prove generated arm guards disjoint for accepted scan shapes',
       'no runtime count for proven programs; incomplete proof rejects or falls back').
option(equal_many, candidate_count_rel,
       'materialize ordinary per-occurrence candidate rows before keyed write',
       'adds TEMP rows and rollback checks; preserves one rel model').

open_question(nested_match_storage,
              'whether compiler-generated nesting helpers should be materialized or inlined').
option(nested_match_storage, authored_rel,
       'author the intermediate rel',
       'queryable table and explicit schema; current behavior').
option(nested_match_storage, generated_rel,
       'compiler generates an ordinary named rel and rules',
       'same SQL graph with compiler-owned name and diagnostic mapping').
option(nested_match_storage, inline_rules,
       'duplicate outer guards into downstream ordinary rules',
       'zero helper table; rule count can multiply').

open_question(scan_surface,
              'whether repeated golden programs justify scan spelling after V6.2').
option(scan_surface, ordinary_rules,
       'retain Event, keyed State, pre, and edge rules',
       'zero syntax and exact lowering remains visible').
option(scan_surface, compiler_generated_rules,
       'a future existing-form declaration expands to ordinary rules',
       'requires a separate user ruling after goldens; zero runtime construct').

receipt(nested_match, pass,
        'two top-level match blocks compose through one ordinary rel and lower to fixed SQL').
receipt(scan_match, pass,
        'one match over an Event Log expands into guarded ordered state edge rules').
receipt(cardinality, pass,
        '0, 1, differing N, and equal N current behaviors are pinned').
receipt(initialization, pass,
        'seed-before-event and event-before-seed timelines are pinned').
receipt(clock, pass,
        'state boundary delta occurs at T and edge listener output at T+1').
receipt(storage, pass,
        'nested match and scan table counts are measured from lower_program/2').
receipt(stale_lab, pass,
        'the former edge_body_needs_pre receipt is reproduced as a false-positive catch binding').
receipt(status, pass,
        'every existing scan and match lab has one disposition').

go :-
    aggregate_all(count, open_question(_,_), QuestionCount),
    QuestionCount =< 5,
    forall(
        open_question(Question, _),
        ( aggregate_all(count, option(Question, _, _, _), OptionCount),
          OptionCount >= 2,
          OptionCount =< 5
        )),
    forall(existing_lab(Name),
           aggregate_all(count, lab_status(Name, _, _), 1)),
    aggregate_all(count, receipt(_, pass, _), 8),
    format("8 receipt records, ~d open questions: PASS~n",
           [QuestionCount]).
