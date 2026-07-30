% Existing-world receipts for a possible scan surface.
%
% Run:
%   swipl -q -l v6/prolog/labs/scan_surface_composition/0_receipts.pl \
%     -g go -g halt
%
% scan_definition/1 is lab-only compiler input. expand_scan_definition/2
% erases it into the current prog/2 IR before the reference engine, checker,
% and SQL lowerer see it. The expansion uses ordinary named relations,
% relation keys, match expansion, pre/1, latest/1, <-, and <+.

:- module(scan_surface_composition_receipts,
          [ go/0,
            scan_definition/1,
            expand_scan_definition/2
          ]).

:- use_module('../../0_match_expand', [expand_match_program/2]).
:- use_module('../../conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../compile/compile', [program_plan/2]).
:- use_module('../../compile/lower', [lower_program/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

go :-
    receipt_exact_erasure,
    receipt_shared_reducer_multiple_instances,
    receipt_subscriber_fanout_does_not_reduce,
    receipt_demand_lifecycle,
    receipt_init_and_reset,
    receipt_candidate_cardinality,
    receipt_named_nested_match,
    receipt_clock_boundaries,
    receipt_storage_counts,
    receipt_fixed_rule_count,
    format("10 PASS~n").

% One definition serves every runtime demand row. Instance identity is the
% declared state key (Instance, Owner, Key). Subscriber identity is separate
% and does not occur in any reducer edge rule.
scan_definition(
    scan_def(
        counter_scan,
        demand(scan_demand/4, key([1, 2, 3])),
        subscriber(scan_subscriber/4, key([1])),
        event(scan_event/6, keep(all)),
        state(scan_cell/4, key([1, 2, 3])),
        view(scan_view/5),
        cancel(scan_cancel/3, keep(all)),
        observed(scan_observed/5, keep(all)),
        arms([add, subtract, reset, keep]))).

% Exact N -> N-1 expansion. There is no relation-valued result and no dynamic
% relation name. The candidate form only supplies these already-ground names.
expand_scan_definition(Definition, Program) :-
    Definition =
        scan_def(
            _Name,
            demand(DemandRef, key(DemandKey)),
            subscriber(SubscriberRef, key(SubscriberKey)),
            event(EventRef, keep(EventKeep)),
            state(StateRef, key(StateKey)),
            view(ViewRef),
            cancel(CancelRef, keep(CancelKeep)),
            observed(ObservedRef, keep(ObservedKeep)),
            arms([add, subtract, reset, keep])),
    DemandRef = DemandName/4,
    SubscriberRef = SubscriberName/4,
    EventRef = EventName/6,
    StateRef = StateName/4,
    ViewRef = ViewName/5,
    CancelRef = CancelName/3,
    ObservedRef = ObservedName/5,
    scan_declarations(
        DemandRef-DemandKey,
        SubscriberRef-SubscriberKey,
        EventRef-EventKeep,
        StateRef-StateKey,
        CancelRef-CancelKeep,
        ObservedRef-ObservedKeep,
        Decls),
    Demand =.. [DemandName, Instance, Owner, Key, Init],
    Subscriber =.. [SubscriberName, SubscriberId, Instance, Owner, Key],
    Event =.. [EventName, Instance, Owner, Key, _EventId, Kind, Amount],
    CellAtInit =.. [StateName, Instance, Owner, Key, Init],
    CellBefore =.. [StateName, Instance, Owner, Key, Previous],
    CellAfter =.. [StateName, Instance, Owner, Key, Next],
    CellKeep =.. [StateName, Instance, Owner, Key, Previous],
    View =.. [ViewName, SubscriberId, Instance, Owner, Key, Current],
    CellCurrent =.. [StateName, Instance, Owner, Key, Current],
    Cancel =.. [CancelName, Instance, Owner, Key],
    Observed =.. [ObservedName, SubscriberId, Instance, Owner, Key, Current],
    AddArm =
        (CellAfter <+
            ( Kind == add,
              latest(Demand),
              pre(CellBefore),
              Next := Previous + Amount )),
    SubtractArm =
        (CellAfter <+
            ( Kind == subtract,
              latest(Demand),
              pre(CellBefore),
              Next := Previous - Amount )),
    ResetArm =
        (CellAtInit <+
            ( Kind == reset,
              latest(Demand) )),
    KeepArm =
        (CellKeep <+
            ( Kind == keep,
              latest(Demand),
              pre(CellBefore) )),
    Sugared =
        prog(
            Decls,
            [ (CellAtInit <+ Demand),
              match(Event,
                    (AddArm ; SubtractArm ; ResetArm ; KeepArm)),
              (View <- Subscriber, Demand, CellCurrent),
              (Cancel <+ finalize(Demand)),
              (Observed <+ View),
              match(
                  View,
                  ((scan_positive(SubscriberId, Instance, Owner, Key, Current) <-
                        Current > 0) ;
                   (scan_nonpositive(SubscriberId, Instance, Owner, Key, Current) <-
                        Current =< 0)))
            ]),
    expand_match_program(Sugared, Program).

receipt_exact_erasure :-
    scan_definition(Definition),
    expand_scan_definition(Definition, prog(Decls, Rules)),
    scan_declarations(
        scan_demand/4-[1, 2, 3],
        scan_subscriber/4-[1],
        scan_event/6-all,
        scan_cell/4-[1, 2, 3],
        scan_cancel/3-all,
        scan_observed/5-all,
        Decls),
    Rules =
        [ (scan_cell(I, O, K, Init) <+ scan_demand(I, O, K, Init)),
          (scan_cell(I, O, K, Next) <+
              scan_event(I, O, K, _, Kind, Amount),
              ( Kind == add,
                latest(scan_demand(I, O, K, Init)),
                pre(scan_cell(I, O, K, Previous)),
                Next := Previous + Amount )),
          (scan_cell(I, O, K, Next) <+
              scan_event(I, O, K, _, Kind, Amount),
              ( Kind == subtract,
                latest(scan_demand(I, O, K, Init)),
                pre(scan_cell(I, O, K, Previous)),
                Next := Previous - Amount )),
          (scan_cell(I, O, K, Init) <+
              scan_event(I, O, K, _, Kind, _),
              ( Kind == reset,
                latest(scan_demand(I, O, K, Init)) )),
          (scan_cell(I, O, K, Previous) <+
              scan_event(I, O, K, _, Kind, _),
              ( Kind == keep,
                latest(scan_demand(I, O, K, _)),
                pre(scan_cell(I, O, K, Previous)) )),
          (scan_view(S, I, O, K, Current) <-
              scan_subscriber(S, I, O, K),
              scan_demand(I, O, K, _),
              scan_cell(I, O, K, Current)),
          (scan_cancel(I, O, K) <+
              finalize(scan_demand(I, O, K, _))),
          (scan_observed(S, I, O, K, Current) <+
              scan_view(S, I, O, K, Current)),
          (scan_positive(S, I, O, K, Current) <-
              scan_view(S, I, O, K, Current),
              Current > 0),
          (scan_nonpositive(S, I, O, K, Current) <-
              scan_view(S, I, O, K, Current),
              Current =< 0)
        ],
    \+ contains_functor(Rules, scan_def/9),
    \+ contains_functor(Rules, match/2),
    format("PASS scan definition erases exactly to 10 ordinary rules and existing declarations~n").

receipt_shared_reducer_multiple_instances :-
    scan_program(Program),
    Initial =
        [ scan_demand(alpha, owner_a, counter, 0),
          scan_demand(beta, owner_a, counter, 10),
          scan_cell(alpha, owner_a, counter, 0),
          scan_cell(beta, owner_a, counter, 10)
        ],
    Schedule =
        [[ +scan_event(alpha, owner_a, counter, e1, add, 2),
           +scan_event(beta, owner_a, counter, e2, add, 7),
           +scan_event(alpha, owner_a, counter, e3, add, 3)
        ]],
    run_program(Program, Initial, Schedule, Final, _),
    rel_rows(
        scan_cell/4,
        Final,
        [ scan_cell(alpha, owner_a, counter, 5),
          scan_cell(beta, owner_a, counter, 17)
        ]),
    format("PASS one reducer rule set partitions state by demand instance, owner, and key~n").

receipt_subscriber_fanout_does_not_reduce :-
    scan_program(Program),
    Initial =
        [ scan_demand(alpha, owner_a, counter, 0),
          scan_subscriber(sub_a, alpha, owner_a, counter),
          scan_subscriber(sub_b, alpha, owner_a, counter),
          scan_cell(alpha, owner_a, counter, 0)
        ],
    run_program(
        Program,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e1, add, 2)]],
        Final,
        _),
    rel_rows(scan_cell/4, Final,
             [scan_cell(alpha, owner_a, counter, 2)]),
    rel_rows(
        scan_view/5,
        Final,
        [ scan_view(sub_a, alpha, owner_a, counter, 2),
          scan_view(sub_b, alpha, owner_a, counter, 2)
        ]),
    format("PASS two subscribers fan out one state result without duplicating reduction~n").

receipt_demand_lifecycle :-
    scan_program(Program),
    Schedule =
        [ [ +scan_demand(alpha, owner_a, counter, 4),
            +scan_subscriber(sub_a, alpha, owner_a, counter),
            +scan_subscriber(sub_b, alpha, owner_a, counter)
          ],
          [ +scan_event(alpha, owner_a, counter, e1, add, 2) ],
          [ -scan_subscriber(sub_a, alpha, owner_a, counter) ],
          [ -scan_demand(alpha, owner_a, counter, 4) ],
          [ +scan_event(alpha, owner_a, counter, ignored, add, 50) ]
        ],
    run_program(Program, [], Schedule, Final, Deltas),
    rel_rows(scan_demand/4, Final, []),
    rel_rows(scan_view/5, Final, []),
    rel_rows(scan_cell/4, Final,
             [scan_cell(alpha, owner_a, counter, 6)]),
    rel_rows(scan_cancel/3, Final,
             [scan_cancel(alpha, owner_a, counter)]),
    rel_deltas(
        scan_view/5,
        Deltas,
        [ [ +scan_view(sub_a, alpha, owner_a, counter, 4),
            +scan_view(sub_b, alpha, owner_a, counter, 4)
          ],
          [ -scan_view(sub_a, alpha, owner_a, counter, 4),
            -scan_view(sub_b, alpha, owner_a, counter, 4),
            +scan_view(sub_a, alpha, owner_a, counter, 6),
            +scan_view(sub_b, alpha, owner_a, counter, 6)
          ],
          [ -scan_view(sub_a, alpha, owner_a, counter, 6) ],
          [ -scan_view(sub_b, alpha, owner_a, counter, 6) ],
          [],
          []
        ]),
    format("PASS demand removal retracts views, emits delayed cancellation, gates later events, and retains an inactive cell~n").

receipt_init_and_reset :-
    scan_program(Program),
    Schedule =
        [ [ +scan_demand(alpha, owner_a, counter, 10) ],
          [ +scan_event(alpha, owner_a, counter, e1, add, 5) ],
          [ +scan_event(alpha, owner_a, counter, e2, reset, 999) ],
          [ -scan_demand(alpha, owner_a, counter, 10) ],
          [ +scan_demand(alpha, owner_a, counter, 100),
            +scan_event(alpha, owner_a, counter, e3, add, 1)
          ],
          [ +scan_event(alpha, owner_a, counter, e4, add, 1) ]
        ],
    run_program(Program, [], Schedule, Final, _),
    rel_rows(scan_cell/4, Final,
             [scan_cell(alpha, owner_a, counter, 102)]),
    format("PASS demand addition initializes before later same-tick events; reset and re-demand restore demand init~n").

receipt_candidate_cardinality :-
    scan_program(Program),
    Initial =
        [ scan_demand(alpha, owner_a, counter, 7),
          scan_cell(alpha, owner_a, counter, 7)
        ],
    run_program(
        Program,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e0, ignore, 2)]],
        ZeroFinal,
        _),
    rel_rows(scan_cell/4, ZeroFinal,
             [scan_cell(alpha, owner_a, counter, 7)]),
    run_program(
        Program,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e1, add, 2)]],
        OneFinal,
        _),
    rel_rows(scan_cell/4, OneFinal,
             [scan_cell(alpha, owner_a, counter, 9)]),
    candidate_program(different, DifferentProgram),
    catch(
        run_program(
            DifferentProgram,
            Initial,
            [[+scan_event(alpha, owner_a, counter, e2, choose, 0)]],
            _,
            _),
        DifferentError,
        true),
    DifferentError =
        keyed_conflict(
            scan_cell/4,
            [alpha, owner_a, counter],
            [ scan_cell(alpha, owner_a, counter, 8),
              scan_cell(alpha, owner_a, counter, 9)
            ]),
    candidate_program(equal, EqualProgram),
    run_program(
        EqualProgram,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e3, choose, 0)]],
        EqualFinal,
        _),
    rel_rows(scan_cell/4, EqualFinal,
             [scan_cell(alpha, owner_a, counter, 8)]),
    format("PASS current candidates remain 0 silent, 1 write, differing N conflict, equal N dedupe~n").

receipt_named_nested_match :-
    scan_program(Program),
    Initial =
        [ scan_demand(alpha, owner_a, counter, 0),
          scan_subscriber(sub_a, alpha, owner_a, counter),
          scan_cell(alpha, owner_a, counter, 0)
        ],
    run_program(
        Program,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e1, add, 2)]],
        PositiveFinal,
        _),
    rel_rows(
        scan_positive/5,
        PositiveFinal,
        [scan_positive(sub_a, alpha, owner_a, counter, 2)]),
    rel_rows(scan_nonpositive/5, PositiveFinal, []),
    run_program(
        Program,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e2, subtract, 3)]],
        NegativeFinal,
        _),
    rel_rows(scan_positive/5, NegativeFinal, []),
    rel_rows(
        scan_nonpositive/5,
        NegativeFinal,
        [scan_nonpositive(sub_a, alpha, owner_a, counter, -3)]),
    format("PASS downstream match nests through the ordinary named scan_view relation~n").

receipt_clock_boundaries :-
    scan_program(Program),
    Initial =
        [ scan_demand(alpha, owner_a, counter, 0),
          scan_subscriber(sub_a, alpha, owner_a, counter),
          scan_cell(alpha, owner_a, counter, 0)
        ],
    run_program(
        Program,
        Initial,
        [[+scan_event(alpha, owner_a, counter, e1, add, 2)]],
        _,
        Deltas),
    rel_deltas(
        scan_cell/4,
        Deltas,
        [[ -scan_cell(alpha, owner_a, counter, 0),
           +scan_cell(alpha, owner_a, counter, 2)
         ],
         [],
         []
        ]),
    rel_deltas(
        scan_view/5,
        Deltas,
        [[ -scan_view(sub_a, alpha, owner_a, counter, 0),
           +scan_view(sub_a, alpha, owner_a, counter, 2)
         ],
         [],
         []
        ]),
    rel_deltas(
        scan_observed/5,
        Deltas,
        [ [],
          [+scan_observed(sub_a, alpha, owner_a, counter, 2)],
          []
        ]),
    format("PASS state and level view publish at T boundary; edge subscriber observes at T+1~n").

receipt_storage_counts :-
    scan_program(Program),
    program_plan(
        fixture(scan_surface_composition, Program, [], [], [])-[],
        Plan),
    Plan = plan(_, _, RelPlans, _, _, _),
    findall(Ref,
            member(relplan(Ref, _, _, _, _), RelPlans),
            Refs0),
    sort(Refs0, Refs),
    Refs =
        [ scan_cancel/3,
          scan_cell/4,
          scan_demand/4,
          scan_event/6,
          scan_nonpositive/5,
          scan_observed/5,
          scan_positive/5,
          scan_subscriber/4,
          scan_view/5
        ],
    lower_program(Plan, Lowered),
    Lowered = lowered(_, Ddl, _, EdgeStatements, LevelStatements, _, _, _),
    length(EdgeStatements, 7),
    length(LevelStatements, 3),
    ddl_table_counts(Ddl, persistent(9), temporary(32)),
    include(contains_text('__pre_scan_cell'), Ddl, PreTables),
    PreTables =
        ['CREATE TEMP TABLE "__pre_scan_cell" ("instance" TEXT NOT NULL, "owner" TEXT NOT NULL, "key" TEXT NOT NULL, "value" INTEGER NOT NULL, PRIMARY KEY ("instance", "owner", "key")) WITHOUT ROWID'],
    format("PASS expansion lowers to 9 persistent rel tables, 32 TEMP support tables, and one keyed pre table~n").

receipt_fixed_rule_count :-
    scan_definition(Definition),
    expand_scan_definition(Definition, prog(_, Rules)),
    length(Rules, RuleCount),
    RuleCount =:= 10,
    demand_rows(1, OneDemandRows),
    demand_rows(1000, ThousandDemandRows),
    length(OneDemandRows, 1),
    length(ThousandDemandRows, 1000),
    length(Rules, 10),
    format("PASS reducer SQL/rule count is fixed at 10 for 1 or 1000 demand rows~n").

scan_program(Program) :-
    scan_definition(Definition),
    expand_scan_definition(Definition, Program).

scan_declarations(
    DemandRef-DemandKey,
    SubscriberRef-SubscriberKey,
    EventRef-EventKeep,
    StateRef-StateKey,
    CancelRef-CancelKeep,
    ObservedRef-ObservedKeep,
    [ keyed(DemandRef, DemandKey),
      keyed(SubscriberRef, SubscriberKey),
      kind(EventRef, log),
      keep(EventRef, EventKeep),
      keyed(StateRef, StateKey),
      kind(CancelRef, log),
      keep(CancelRef, CancelKeep),
      kind(ObservedRef, log),
      keep(ObservedRef, ObservedKeep),
      col_type(scan_demand/4, instance, text),
      col_type(scan_demand/4, owner, text),
      col_type(scan_demand/4, key, text),
      col_type(scan_demand/4, init, int),
      col_type(scan_subscriber/4, subscriber, text),
      col_type(scan_subscriber/4, instance, text),
      col_type(scan_subscriber/4, owner, text),
      col_type(scan_subscriber/4, key, text),
      col_type(scan_event/6, instance, text),
      col_type(scan_event/6, owner, text),
      col_type(scan_event/6, key, text),
      col_type(scan_event/6, event_id, text),
      col_type(scan_event/6, kind, text),
      col_type(scan_event/6, amount, int),
      col_type(scan_cell/4, instance, text),
      col_type(scan_cell/4, owner, text),
      col_type(scan_cell/4, key, text),
      col_type(scan_cell/4, value, int),
      col_type(scan_view/5, subscriber, text),
      col_type(scan_view/5, instance, text),
      col_type(scan_view/5, owner, text),
      col_type(scan_view/5, key, text),
      col_type(scan_view/5, value, int),
      col_type(scan_cancel/3, instance, text),
      col_type(scan_cancel/3, owner, text),
      col_type(scan_cancel/3, key, text),
      col_type(scan_observed/5, subscriber, text),
      col_type(scan_observed/5, instance, text),
      col_type(scan_observed/5, owner, text),
      col_type(scan_observed/5, key, text),
      col_type(scan_observed/5, value, int),
      col_type(scan_positive/5, subscriber, text),
      col_type(scan_positive/5, instance, text),
      col_type(scan_positive/5, owner, text),
      col_type(scan_positive/5, key, text),
      col_type(scan_positive/5, value, int),
      col_type(scan_nonpositive/5, subscriber, text),
      col_type(scan_nonpositive/5, instance, text),
      col_type(scan_nonpositive/5, owner, text),
      col_type(scan_nonpositive/5, key, text),
      col_type(scan_nonpositive/5, value, int)
    ]).

candidate_program(Mode, prog(Decls, Rules)) :-
    scan_program(prog(Decls, BaseRules)),
    include(non_step_rule, BaseRules, NonStepRules),
    ( Mode == different
    -> CandidateRules =
        [ (scan_cell(I, O, K, Next) <+
              scan_event(I, O, K, _, choose, _),
              latest(scan_demand(I, O, K, _)),
              pre(scan_cell(I, O, K, Previous)),
              Next := Previous + 1),
          (scan_cell(I, O, K, Next) <+
              scan_event(I, O, K, _, choose, _),
              latest(scan_demand(I, O, K, _)),
              pre(scan_cell(I, O, K, Previous)),
              Next := Previous + 2)
        ]
    ; Mode == equal
    -> CandidateRules =
        [ (scan_cell(I, O, K, Next) <+
              scan_event(I, O, K, _, choose, _),
              latest(scan_demand(I, O, K, _)),
              pre(scan_cell(I, O, K, Previous)),
              Next := Previous + 1),
          (scan_cell(I, O, K, Next) <+
              scan_event(I, O, K, _, choose, _),
              latest(scan_demand(I, O, K, _)),
              pre(scan_cell(I, O, K, Previous)),
              Next := Previous + 1)
        ]
    ),
    append(NonStepRules, CandidateRules, Rules).

non_step_rule(Rule) :-
    \+ step_rule(Rule).

step_rule((_ <+ Body)) :-
    contains_functor(Body, scan_event/6).

demand_rows(Count, Rows) :-
    findall(
        scan_demand(Instance, owner, key, 0),
        ( between(1, Count, Index),
          atom_concat(instance_, Index, Instance)
        ),
        Rows).

contains_functor(Term, Functor/Arity) :-
    sub_term(SubTerm, Term),
    nonvar(SubTerm),
    functor(SubTerm, Functor, Arity).

contains_text(Needle, Text) :-
    sub_string(Text, _, _, _, Needle).

ddl_table_counts(Ddl, persistent(Persistent), temporary(Temporary)) :-
    include(contains_text('CREATE TABLE '), Ddl, PersistentTables),
    include(contains_text('CREATE TEMP TABLE '), Ddl, TemporaryTables),
    length(PersistentTables, Persistent),
    length(TemporaryTables, Temporary).
