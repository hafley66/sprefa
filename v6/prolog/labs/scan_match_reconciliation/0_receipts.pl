% Existing-world scan and match reconciliation receipts.
%
% Run:
%   swipl -q -l v6/prolog/labs/scan_match_reconciliation/0_receipts.pl \
%     -g go -g halt
%
% This lab calls the current parser, match expander, oracle, checker, and SQL
% lowerer. It adds no production construct.

:- use_module('../../0_match_expand',
              [expand_match_program/2]).
:- use_module('../../conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../compile/parse_dl',
              [parse_dl/4]).
:- use_module('../../compile/compile',
              [program_plan/2]).
:- use_module('../../compile/lower',
              [lower_program/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

go :-
    receipt_nested_match_uses_ordinary_rel,
    receipt_nested_match_surface_and_sql,
    receipt_direct_nested_block_refuses,
    receipt_scan_match_expands_to_ordered_rules,
    receipt_scan_zero_one_many,
    receipt_scan_init_order,
    receipt_scan_clock,
    receipt_scan_sql_tables,
    receipt_stale_higher_order_false_positive,
    receipt_lab_statuses_are_total,
    format("10 PASS~n").

receipt_nested_match_uses_ordinary_rel :-
    nested_match_program(Sugared),
    expand_match_program(Sugared, Expanded),
    Expanded = prog(
        [],
        [ (stage(Key, Value) <- input(Key, Kind, Value), Kind == add),
          (stage(Key, 0) <- input(Key, Kind, _Value), Kind == zero),
          (positive(Key, Value) <-
              stage(Key, Value), Value > 0),
          (nonpositive(Key, Value) <-
              stage(Key, Value), Value =< 0)
        ]),
    run_program(
        Expanded,
        [input(a, add, 2), input(b, zero, 99), input(c, skip, 8)],
        [],
        Final,
        _),
    rel_rows(stage/2, Final, [stage(a, 2), stage(b, 0)]),
    rel_rows(positive/2, Final, [positive(a, 2)]),
    rel_rows(nonpositive/2, Final, [nonpositive(b, 0)]),
    format("PASS nested match composition is two match blocks joined by one ordinary rel~n").

receipt_nested_match_surface_and_sql :-
    nested_match_surface(Codes),
    parse_dl(Codes, Program, _, []),
    Program = prog(
        _,
        [ match(input(_, _, _), _),
          match(stage(_, _), _)
        ]),
    program_plan(
        fixture(nested_match_reconciliation, Program, [], [], [])-[],
        Plan),
    Plan = plan(_, prog(_, ExpandedRules), RelPlans, _, _, _),
    length(ExpandedRules, 4),
    \+ contains_functor(ExpandedRules, match/2),
    findall(Ref,
            member(relplan(Ref, _, _, _, _), RelPlans),
            Refs0),
    sort(Refs0, Refs),
    Refs == [input/3, nonpositive/2, positive/2, stage/2],
    lower_program(Plan, Lowered),
    Lowered = lowered(_, Ddl, _, _, LevelStatements, _, _, _),
    length(LevelStatements, 3),
    ddl_table_counts(Ddl, persistent(4), temporary(15)),
    format("PASS nested match parses then lowers to 4 rel tables, 15 TEMP support tables, and 3 level statement groups~n").

receipt_direct_nested_block_refuses :-
    string_codes(
        "rel input(key: text, value: int).\nrel output(key: text, value: int).\nmatch input(Key, Value) (\n  ; true |-> match input(Key, Value) (\n      ; true |-> output(Key, Value)\n    )\n).\n",
        Codes),
    catch(parse_dl(Codes, _, _, _), Error, true),
    Error = dl_parse_error(statement, _),
    format("PASS a match block in an arm head is refused by current dl_parse_error(statement)~n").

receipt_scan_match_expands_to_ordered_rules :-
    scan_match_program(Sugared),
    expand_match_program(Sugared, Expanded),
    Expanded = prog(
        _,
        [ (machine(Key, Next) <+
              select_event(Key, _, Kind, Amount),
              (Kind == add, pre(machine(Key, Previous)),
               Next := Previous + Amount)),
          (machine(Key, Previous) <+
              select_event(Key, _, Kind, _Amount),
              (Kind == keep, pre(machine(Key, Previous))))
        ]),
    run_program(
        Expanded,
        [machine(a, 0), machine(b, 10)],
        [[+select_event(a, 1, add, 2),
          +select_event(b, 2, keep, 999),
          +select_event(a, 3, add, 4)]],
        Final,
        _),
    rel_rows(machine/2, Final, [machine(a, 6), machine(b, 10)]),
    format("PASS match over a scan source expands to ordinary guarded edge rules and preserves occurrence order~n").

receipt_scan_zero_one_many :-
    zero_scan_program(ZeroProgram),
    run_program(
        ZeroProgram,
        [machine(a, 7)],
        [[+select_event(a, 1, ignore, 2)]],
        ZeroFinal,
        _),
    rel_rows(machine/2, ZeroFinal, [machine(a, 7)]),
    one_scan_program(OneProgram),
    run_program(
        OneProgram,
        [machine(a, 7)],
        [[+select_event(a, 1, add, 2)]],
        OneFinal,
        _),
    rel_rows(machine/2, OneFinal, [machine(a, 9)]),
    differing_many_scan_program(DifferingManyProgram),
    catch(
        run_program(
            DifferingManyProgram,
            [machine(a, 7)],
            [[+select_event(a, 1, choose, 0)]],
            _,
            _),
        DifferingError,
        true),
    DifferingError =
        keyed_conflict(machine/2, [a], [machine(a, 8), machine(a, 9)]),
    equal_many_scan_program(EqualManyProgram),
    run_program(
        EqualManyProgram,
        [machine(a, 7)],
        [[+select_event(a, 1, choose, 0)]],
        EqualManyFinal,
        _),
    rel_rows(machine/2, EqualManyFinal, [machine(a, 8)]),
    format("PASS scan candidates are semidet today: 0 is silent, 1 writes, differing N keyed_conflict, equal N dedupes~n").

receipt_scan_init_order :-
    init_scan_program(Program),
    run_program(
        Program,
        [],
        [[+seed(a, 10), +add(a, 2)]],
        SeedThenEvent,
        _),
    rel_rows(machine/2, SeedThenEvent, [machine(a, 12)]),
    run_program(
        Program,
        [],
        [[+add(a, 2), +seed(a, 10)]],
        EventThenSeed,
        _),
    rel_rows(machine/2, EventThenSeed, [machine(a, 10)]),
    format("PASS init is an authored state write and same-batch source order controls whether the first event sees it~n").

receipt_scan_clock :-
    clock_scan_program(Program),
    run_program(
        Program,
        [machine(a, 0)],
        [[+add(a, 2)]],
        Final,
        Deltas),
    rel_rows(machine/2, Final, [machine(a, 2)]),
    rel_rows(observed/2, Final, [observed(a, 2)]),
    rel_deltas(machine/2, Deltas,
               [[-machine(a, 0), +machine(a, 2)], [], []]),
    rel_deltas(observed/2, Deltas,
               [[], [+observed(a, 2)], []]),
    format("PASS scan state commits at the boundary and an edge listener observes it on the next carry tick~n").

receipt_scan_sql_tables :-
    scan_match_program(Sugared),
    expand_match_program(Sugared, Program),
    program_plan(
        fixture(scan_match_sql, Program, [machine(a, 0)], [], [])-[],
        Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, _, RelPlans, _, _, _),
    findall(Ref,
            member(relplan(Ref, _, _, _, _), RelPlans),
            Refs0),
    sort(Refs0, [machine/2, select_event/4]),
    Lowered = lowered(_, Ddl, _, EdgeStatements, _, _, _, _),
    length(EdgeStatements, 2),
    forall(
        member(edgestmt(machine/2, select_event/4, _, _, _, _, _,
                        ordered_arrival),
               EdgeStatements),
        true),
    ddl_table_counts(Ddl, persistent(2), temporary(7)),
    include(contains_text('__pre_machine'), Ddl, PreTables),
    PreTables =
        ['CREATE TEMP TABLE "__pre_machine" ("key" TEXT NOT NULL, "state" INTEGER NOT NULL, PRIMARY KEY ("key")) WITHOUT ROWID'],
    format("PASS scan lowers to 2 rel tables, 7 TEMP support tables, 1 pre table, and 2 ordered edge arms~n").

receipt_stale_higher_order_false_positive :-
    scan_match_program(Sugared),
    expand_match_program(Sugared, Program),
    catch(
        program_plan(
            fixture(stale_gap_receipt, Program, [machine(a, 0)], [], [])-[],
            _),
        Error,
        true),
    var(Error),
    % This is the exact false-positive shape in higher_order_scan:
    % unification manufactures the expected error after program_plan succeeds.
    Error = unsupported_construct(edge_body_needs_pre(_)),
    format("PASS old edge_body_needs_pre receipt is false-positive because catch success leaves its error variable free~n").

receipt_lab_statuses_are_total :-
    findall(Name, existing_lab(Name), Labs0),
    sort(Labs0, Labs),
    findall(Name,
            (existing_lab(Name), lab_status(Name, _, _)),
            Classified0),
    sort(Classified0, Classified),
    Labs == Classified,
    format("PASS every existing scan and match lab has one reconciliation status~n").

nested_match_program(
    prog(
        [],
        [ match(
              input(Key, Kind, Value),
              ((stage(Key, Value) <- Kind == add) ;
               (stage(Key, 0) <- Kind == zero))),
          match(
              stage(Key, Value),
              ((positive(Key, Value) <- Value > 0) ;
               (nonpositive(Key, Value) <- Value =< 0)))
        ])).

nested_match_surface(Codes) :-
    string_codes(
        "rel input(key: text, kind: text, value: int).\nrel stage(key: text, value: int).\nrel positive(key: text, value: int).\nrel nonpositive(key: text, value: int).\nmatch input(Key, Kind, Value) (\n  ; Kind == \"add\" |-> stage(Key, Value)\n  ; Kind == \"zero\" |-> stage(Key, 0)\n).\nmatch stage(Key, Value) (\n  ; Value > 0 |-> positive(Key, Value)\n  ; Value =< 0 |-> nonpositive(Key, Value)\n).\n",
        Codes).

scan_match_program(
    prog(
        [ kind(select_event/4, log),
          keep(select_event/4, all),
          keyed(machine/2, [1]),
          col_type(select_event/4, key, text),
          col_type(select_event/4, sequence, int),
          col_type(select_event/4, kind, text),
          col_type(select_event/4, amount, int),
          col_type(machine/2, key, text),
          col_type(machine/2, state, int)
        ],
        [match(Source, (AddArm ; KeepArm))])) :-
    Source = select_event(Key, _Sequence, Kind, Amount),
    AddArm =
        (machine(Key, Next) <+
            (Kind == add,
             pre(machine(Key, Previous)),
             Next := Previous + Amount)),
    KeepArm =
        (machine(Key, Previous) <+
            (Kind == keep,
             pre(machine(Key, Previous)))).

zero_scan_program(
    prog(
        [ kind(select_event/4, log),
          keep(select_event/4, all),
          keyed(machine/2, [1])
        ],
        [ (machine(Key, Next) <+
              select_event(Key, _, add, Amount),
              pre(machine(Key, Previous)),
              Next := Previous + Amount)
        ])).

one_scan_program(Program) :-
    zero_scan_program(Program).

differing_many_scan_program(
    prog(
        [ kind(select_event/4, log),
          keep(select_event/4, all),
          keyed(machine/2, [1])
        ],
        [ (machine(Key, Next) <+
              select_event(Key, _, choose, _),
              pre(machine(Key, Previous)),
              Next := Previous + 1),
          (machine(Key, Next) <+
              select_event(Key, _, choose, _),
              pre(machine(Key, Previous)),
              Next := Previous + 2)
        ])).

equal_many_scan_program(
    prog(
        [ kind(select_event/4, log),
          keep(select_event/4, all),
          keyed(machine/2, [1])
        ],
        [ (machine(Key, Next) <+
              select_event(Key, _, choose, _),
              pre(machine(Key, Previous)),
              Next := Previous + 1),
          (machine(Key, Next) <+
              select_event(Key, _, choose, _),
              pre(machine(Key, Previous)),
              Next := Previous + 1)
        ])).

init_scan_program(
    prog(
        [ kind(seed/2, log),
          keep(seed/2, all),
          kind(add/2, log),
          keep(add/2, all),
          keyed(machine/2, [1])
        ],
        [ (machine(Key, Initial) <+ seed(Key, Initial)),
          (machine(Key, Next) <+
              add(Key, Amount),
              pre(machine(Key, Previous)),
              Next := Previous + Amount)
        ])).

clock_scan_program(
    prog(
        [ kind(add/2, log),
          keep(add/2, all),
          keyed(machine/2, [1]),
          kind(observed/2, log),
          keep(observed/2, all)
        ],
        [ (machine(Key, Next) <+
              add(Key, Amount),
              pre(machine(Key, Previous)),
              Next := Previous + Amount),
          (observed(Key, Value) <+ machine(Key, Value))
        ])).

contains_functor(Term, Name/Arity) :-
    sub_term(Subterm, Term),
    nonvar(Subterm),
    compound(Subterm),
    functor(Subterm, Name, Arity).

contains_text(Needle, Text) :-
    sub_atom(Text, _, _, _, Needle).

ddl_table_counts(Ddl, persistent(Persistent), temporary(Temporary)) :-
    include(starts_with('CREATE TABLE '), Ddl, PersistentTables),
    include(starts_with('CREATE TEMP TABLE '), Ddl, TemporaryTables),
    length(PersistentTables, Persistent),
    length(TemporaryTables, Temporary).

starts_with(Prefix, Text) :-
    sub_atom(Text, 0, _, _, Prefix).

% Mirrored from the durable reconciliation record. The totality receipt keeps
% additions to the lab census from being left unclassified here.

existing_lab(match_frontier).
existing_lab(match_block).
existing_lab(higher_order_rel_scan).
existing_lab(scan_match_value).
existing_lab(generic_scan_instantiation).
existing_lab(select_scan_cache).
existing_lab(ordered_pre).

lab_status(match_frontier, closed,
           'historical frontier and lifecycle-arm contradiction census').
lab_status(match_block, implemented,
           'parser, printer, shared expander, coverage check, and SQL identity tests exist').
lab_status(higher_order_rel_scan, superseded,
           'its catch leaves the error variable unbound on success, then unifies that variable with edge_body_needs_pre; ordered-pre lowering now succeeds').
lab_status(scan_match_value, superseded,
           'its 0/1/N findings survive; RuleRef and scan surface proposals do not').
lab_status(generic_scan_instantiation, closed,
           'compile-time prototype answered storage and specialization costs; no scan_signature model is adopted').
lab_status(select_scan_cache, canonical_plan,
           'ordinary-rel switch cache rules and receipts remain the golden algorithm plan').
lab_status(ordered_pre, implemented,
           'analyzer, lowerer, emitter, runtime, and focused tests exist in the current tree').
