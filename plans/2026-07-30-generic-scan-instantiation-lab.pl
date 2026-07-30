% Generic scan instantiation lab.
%
% Consult:
%   swipl -q -l plans/2026-07-30-generic-scan-instantiation-lab.pl
%
% Queries:
%   ?- finding(Name, Evidence).
%   ?- lowering_step(N, Input, Output).
%   ?- choice(Question, Route, Price, Result).
%   ?- choice(Question, Route, Cost, Effect).
%
% Runnable receipts:
%   swipl -q -l v6/prolog/labs/generic_scan_instantiation/0_receipts.pl \
%     -g go -g halt

:- module(generic_scan_instantiation_plan,
          [ goal/2,
            finding/2,
            lowering_step/3,
            planner_boundary/3,
            storage_cost/3,
            value_support/3,
            choice/4,
            receipt/3,
            next_step/2
          ]).

:- discontiguous choice/4.

goal(scan,
     'instantiate one ordered keyed state reduction from concrete named event, state, init, and step relations').
goal(generics,
     'reuse one scan planner through compile-time specialization; runtime rows never contain relation or function values').
goal(reversibility,
     'keep the first implementation behind a built-in compiler slot; ordinary handwritten rules remain the N-1 form').

finding(first_order_composition,
        'A <- B and A <- B,C already compose concrete relations by unification').
finding(higher_order_remainder,
        'choosing EventRel, StateRel, InitRel, and StepRel as arguments to one reusable scan algorithm is the remaining compile-time higher-order operation').
finding(state_identity,
        'the minimal scan names StateRel explicitly; scan creates no anonymous persistent relation').
finding(step_identity,
        'StepRel names a set of rule templates; specialization clones their bodies with concrete state and event slots, then erases StepRel').
finding(current_init_behavior,
        'an event with no matching state row currently yields zero writes; the strict scan contract still needs a named missing-init failure').
finding(current_cardinality_gap,
        'the current checked IR has no det/semidet/multi field and no per-event candidate-count transaction check').
finding(ordered_runtime,
        'the in-flight ordered-pre implementation supplies the N-1 loop; this lab does not claim that implementation is integrated or complete').

% Compiler-facing input. This is a relation in the planning program.
%
% scan_spec(
%   Name,
%   EventRel,
%   StateRel,
%   InitRel,
%   StepRel,
%   key_map(
%     event(KeyPositions, PayloadPositions),
%     state(KeyPositions, ValuePositions),
%     init(KeyPositions, ValuePositions),
%     step(PreviousPositions, EventPositions, NextPositions)))
% ).

lowering_step(0, scan_spec,
              'validate every relation name is ground and resolves to one program definition').
lowering_step(1, relation_definitions,
              'read arity, columns, column types, kind, and key positions').
lowering_step(2, key_map,
              'unify event, state, and init key types and arities').
lowering_step(3, type_slots,
              'substitute concrete state and event types into the StepRel signature').
lowering_step(4, clock_signature,
              'require a pure grade-0 step; derive boundary state write and grade-1 downstream observation').
lowering_step(5, definition_hashes,
              'hash canonical schemas plus step rules; deduplicate identical specializations').
lowering_step(6, init_rule,
              'generate State(Key,Initial) <+ Init(Key,Initial)').
lowering_step(7, step_rules,
              'inline every StepRel body into State(Key,Next) <+ Event(...), pre(State(Key,Previous)), Body').
lowering_step(8, checked_program,
              'erase scan_spec and run the existing checker, lowerer, and SQLite emitter').

planner_boundary(datalog,
                 'derive validation facts, type substitutions, clock signatures, reuse keys, required rules, and required storage',
                 'all outputs remain finite relations over program metadata').
planner_boundary(prolog_compiler,
                 'copy rule terms, allocate fresh variables, construct predicate terms with =.., and splice generated rules into prog/2',
                 'term synthesis is a compiler action even when its plan was derived relationally').
planner_boundary(sqlite_runtime,
                 'execute only the specialized named relations and ordered state rules',
                 'no scan, function, RuleRef, or relation value survives').

storage_cost(named_scan, '0 generated persistent tables',
             'EventRel, StateRel, and InitRel are existing caller declarations; StepRel is inlined').
storage_cost(ordered_state, '1 TEMP pre table per referenced StateRel',
             'runtime support belongs to the ordered-pre lowering, independent of generic specialization count').
storage_cost(separate_state, '1 authored StateRel per independent state machine',
             'different StateRel names prevent sharing').
storage_cost(shared_state, '1 authored StateRel for all contributing event relations',
             'multiple specialized step rules target the same key space and share arrival order').
storage_cost(nested_scan, '1 authored child StateRel keyed by parent key plus child key',
             'nesting is a product key, not a runtime graph value').

value_support(int,
              'working',
              '+, -, *, truncating /, mod, <, =<, >, >=, ==, and \\== lower through the current expression registry').
value_support(text,
              'partial',
              'storage, equality, and concat work; general string methods remain outside the first scan contract').
value_support(bool,
              'pending',
              'relation membership and guards express control; a stored strict bool type remains phase-5 work').
value_support(float,
              'pending',
              'REAL storage, arithmetic, comparison, avg, and precision rules remain phase-5 work').
value_support(objects,
              'partial',
              'declared relational rows and endpoint IDs are usable; dictionary and JSON value paths are excluded').
value_support(lists,
              'pending',
              'operational list fan-out is outside the first scan contract; collections stay relations').

% Exactly three unresolved decisions, each with three priced routes.

choice(state_instantiation, explicit_named,
       'author declares StateRel and its key; specialization adds rules only',
       'queryable stable storage identity and zero anonymous tables').
choice(state_instantiation, content_named,
       'compiler creates one hash-named StateRel and must own migrations and diagnostics',
       'less authoring with one generated persistent table per distinct state specialization').
choice(state_instantiation, runtime_dynamic,
       'requires executable relation values, dynamic DDL, lifetime ownership, and weaker whole-graph checks',
       'a row may instantiate state at runtime').

choice(initialization, strict_required,
       'add a per-event state-membership validation and roll back the tick on zero or many initial states',
       'every accepted event reduces exactly once').
choice(initialization, lazy_init,
       'add InitRule(State,Event,Initial) and define first-event ordering',
       'the first event may construct state').
choice(initialization, silent_zero,
       'keep the current semidet join',
       'an event before initialization disappears').

choice(reducer_cardinality, prove_or_validate,
       'prove det for known match shapes; otherwise count candidates before applying writes and roll back on zero or many',
       'match remains relational while scan requires one next state').
choice(reducer_cardinality, single_rule_only,
       'accept one syntactic StepRel rule with no multi-row joins',
       'small implementation with no general scan + match').
choice(reducer_cardinality, priority_first,
       'make arm order select one result',
       'changes match union semantics and adds ordered-choice behavior').

receipt(relational_planner, pass,
        'scan_spec facts derive concrete plans, types, clocks, storage, and rules').
receipt(real_oracle, pass,
        'specialized integer scan folds duplicate events in order and partitions by key').
receipt(real_compiler, pass,
        'specialized scan reaches current program_plan/2 and lower_program/2 with scan erased').
receipt(reuse, pass,
        'two identical call sites share one specialization hash').
receipt(state_sharing, pass,
        'separate StateRel names yield two tables; two scans targeting one StateRel yield one table').
receipt(nested, pass,
        'a composite-key child scan specializes and runs without a runtime graph value').
receipt(refusals, pass,
        'unknown init, dynamic relation name, recursive reducer, type mismatch, and known multi reducer are named before lowering').

next_step(1,
          'finish and grade ordered-pre production work before integrating scan specialization').
next_step(2,
          'add scan_spec facts to compiler IR and the expansion pass; add no .dl6 syntax').
next_step(3,
          'add strict missing-init and reducer-cardinality rollback receipts').
next_step(4,
          'write one ghcacher or extraction state machine using handwritten N-1 rules and compare its specialized graph').
next_step(5,
          'select surface syntax only after two golden programs reuse the built-in slot').
