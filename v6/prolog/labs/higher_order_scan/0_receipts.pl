% 0_receipts.pl : compile-time named-rule/scan specialization lab.
%
% This file is deliberately outside the production parser/compiler.  The
% small specializer below accepts named algorithm calls, emits the existing
% prog/2 IR, then runs that IR through the shared oracle and compiler plan.
% No function value survives in the emitted program.
%
% Run:
%   swipl -q -l v6/prolog/labs/higher_order_scan/0_receipts.pl -g go -g halt

:- use_module('../../conformance/engine', [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../compile/compile', [program_plan/2, compile_program/6]).
:- use_module('../../compile/lower', [lower_program/2]).
:- use_module('../../compile/parse_dl', [parse_dl/4, parse_dl_file/4]).
:- use_module('../../1_host_expand', [prepare_program/5]).
:- use_module(library(readutil), [read_file_to_string/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Canonical signature boundary. Explicit declarations can construct this term
% now. Later inference must construct the same term before specialization.
canonical_signature(
    add_step,
    sig(
        inputs([state(any/2), event(any/3)]),
        outputs([state(any/2)]),
        reads([state, event]),
        writes([state]),
        grade(write(0), observe(1)),
        cardinality(at_most_one_per_occurrence_per_key),
        lifetime(keyed_state),
        effects([]))).

canonical_signature(
    switch_inner,
    sig(
        inputs([event(any/2), result(any/2)]),
        outputs([view(any/2)]),
        reads([event, result, scope]),
        writes([scope]),
        grade(write(0), observe(1), async_result(at_least(1))),
        cardinality(at_most_one_scope_per_key),
        lifetime(until_next_outer_with_same_key),
        effects([demand_response]))).

canonical_signature(
    switch_scan_step,
    sig(
        inputs([context(any/2), event(any/2), state(any/3)]),
        outputs([state(any/3)]),
        reads([context, event, state]),
        writes([state]),
        grade(write(0), observe(1)),
        cardinality(at_most_one_per_occurrence_per_key),
        lifetime(until_next_context_with_same_key),
        effects([]))).

% Tier-N call -> existing tier-(N-1) prog/2.
specialize(
    scan(EventName/3, StateName/2, add_step, key([1])),
    Signature,
    prog(
        [ kind(EventName/3, log),
          keep(EventName/3, all),
          keyed(StateName/2, [1])
        ],
        [ Rule ])) :-
    canonical_signature(add_step, Signature),
    Event =.. [EventName, Key, _EventId, Amount],
    Previous =.. [StateName, Key, Old],
    NextState =.. [StateName, Key, Next],
    Rule = (NextState <+ Event, pre(Previous), Next := Old + Amount).

specialize(
    switch_map(
        OuterName/2, ScopeName/2, ResultName/2, ViewName/2, key([1])),
    Signature,
    prog(
        [ kind(OuterName/2, log),
          keep(OuterName/2, all),
          kind(ResultName/2, log),
          keep(ResultName/2, all),
          keyed(ScopeName/2, [1])
        ],
        [ ScopeRule, DemandRule, ViewRule ])) :-
    canonical_signature(switch_inner, Signature),
    Outer =.. [OuterName, Owner, Target],
    Scope =.. [ScopeName, Owner, Target],
    Result =.. [ResultName, Target, Value],
    Demand = demanded(Target, Owner),
    View =.. [ViewName, Target, Value],
    ScopeRule = (Scope <+ Outer),
    DemandRule = (Demand <- Scope),
    ViewRule = (View <- Demand, Result).

specialize(
    switch_scan(
        ContextName/2, EventName/2, StateName/3, switch_scan_step, key([1])),
    Signature,
    prog(
        [ kind(ContextName/2, log),
          keep(ContextName/2, all),
          kind(EventName/2, log),
          keep(EventName/2, all),
          keyed(StateName/3, [1])
        ],
        [ ResetRule, StepRule, ViewRule ])) :-
    canonical_signature(switch_scan_step, Signature),
    Context =.. [ContextName, Owner, ContextId],
    ResetState =.. [StateName, Owner, ContextId, 0],
    Event =.. [EventName, Owner, Amount],
    Previous =.. [StateName, Owner, ActiveContext, Old],
    NextState =.. [StateName, Owner, ActiveContext, Next],
    View = state_view(Owner, ActiveContext, Next),
    ResetRule = (ResetState <+ Context),
    StepRule = (NextState <+ Event, pre(Previous), Next := Old + Amount),
    ViewRule = (View <- NextState).

go :-
    scan_partition_receipt,
    switch_map_receipt,
    switch_scan_receipt,
    specialization_erasure_receipt,
    compiler_boundary_receipt,
    scan_compiler_gap_receipt,
    current_surface_receipt,
    format("7 PASS~n").

scan_partition_receipt :-
    Call = scan(add/3, total/2, add_step, key([1])),
    specialize(Call, _, Prog),
    Initial = [total(a, 0), total(b, 10)],
    Schedule = [[+add(a, e1, 1), +add(b, e2, 2), +add(a, e3, 3)]],
    run_program(Prog, Initial, Schedule, Final, Deltas),
    rel_rows(total/2, Final, [total(a, 4), total(b, 12)]),
    rel_deltas(
        total/2,
        Deltas,
        [[-total(a, 0), -total(b, 10), +total(a, 4), +total(b, 12)], []]),
    format("PASS scan partitioned by key, ordered same-tick occurrences~n").

switch_map_receipt :-
    Call = switch_map(
        route_change/2, open_scope/2, route_result/2, route_view/2, key([1])),
    specialize(Call, _, Prog),
    Schedule = [
        [+route_change(session, settings)],
        [+route_result(settings, settings_body)],
        [+route_change(session, profile)],
        [+route_result(settings, late_settings_body)],
        [+route_result(profile, profile_body)]
    ],
    run_program(Prog, [], Schedule, Final, Deltas),
    rel_rows(route_view/2, Final, [route_view(profile, profile_body)]),
    rel_deltas(
        route_view/2,
        Deltas,
        [[], [+route_view(settings, settings_body)],
         [-route_view(settings, settings_body)], [], [+route_view(profile, profile_body)]]),
    format("PASS switchMap keyed replacement retracts old view and rejects late result~n").

switch_scan_receipt :-
    Call = switch_scan(
        page_change/2, page_event/2, machine/3, switch_scan_step, key([1])),
    specialize(Call, _, Prog),
    Schedule = [
        [+page_change(session, page_a), +page_event(session, 2), +page_event(session, 3)],
        [+page_event(session, 1)],
        [+page_change(session, page_b), +page_event(session, 4)]
    ],
    run_program(Prog, [], Schedule, Final, Deltas),
    rel_rows(machine/3, Final, [machine(session, page_b, 4)]),
    rel_deltas(
        machine/3,
        Deltas,
        [[+machine(session, page_a, 5)],
         [-machine(session, page_a, 5), +machine(session, page_a, 6)],
         [-machine(session, page_a, 6), +machine(session, page_b, 4)], []]),
    format("PASS switchScan resets context then scans later occurrences in queue order~n").

specialization_erasure_receipt :-
    forall(
        member(
            Call,
            [ scan(add/3, total/2, add_step, key([1])),
              switch_map(route_change/2, open_scope/2, route_result/2,
                         route_view/2, key([1])),
              switch_scan(page_change/2, page_event/2, machine/3,
                          switch_scan_step, key([1]))
            ]),
        ( specialize(Call, _, Program),
          \+ forbidden_higher_order_subterm(Program)
        )),
    format("PASS specialization erases rule and relation values~n").

forbidden_higher_order_subterm(Program) :-
    sub_term(Term, Program),
    nonvar(Term),
    functor(Term, Functor, _),
    memberchk(Functor, [scan, switch_map, switch_scan, sig]).

% switchMap's specialized graph reaches the actual compiler/lowerer.  scan and
% switchScan intentionally stop at the current named pre-occurrence refusal.
compiler_boundary_receipt :-
    Call = switch_map(
        route_change/2, open_scope/2, route_result/2, route_view/2, key([1])),
    specialize(Call, _, Prog),
    program_plan(
        fixture(higher_order_switch_map, Prog, [], [], [])-[],
        Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, _, RelPlans, _, _, _),
    member(relplan(open_scope/2, set, _, key([1]), _), RelPlans),
    Lowered = lowered(_, Ddl, _, EdgeStatements, _, _, _, _),
    member(edgestmt(open_scope/2, route_change/2, _, _, _, _, _, _),
           EdgeStatements),
    member(DdlStatement, Ddl),
    sub_string(DdlStatement, _, _, _, "open_scope"),
    tmp_file_stream(text, EmittedFile, EmittedStream),
    close(EmittedStream),
    setup_call_cleanup(
        true,
        ( with_output_to(
              string(_),
              compile_program(
                  higher_order_switch_map,
                  fixture(higher_order_switch_map, Prog, [], [], []),
                  [],
                  [],
                  EmittedFile,
                  emit_ts:emit_program)),
          read_file_to_string(EmittedFile, EmittedText, []),
          sub_string(EmittedText, _, _, _, "open_scope"),
          \+ sub_string(EmittedText, _, _, _, "switchMap("),
          \+ sub_string(EmittedText, _, _, _, "scan(")
        ),
        delete_file(EmittedFile)),
    format("PASS switchMap specializes through real plan and SQL lowering~n").

scan_compiler_gap_receipt :-
    specialize(scan(add/3, total/2, add_step, key([1])), _, ScanProg),
    catch(
        program_plan(fixture(scan_gap, ScanProg, [total(a, 0)], [], [])-[], _),
        ScanError,
        true),
    ScanError = unsupported_construct(edge_body_needs_pre(_)),
    specialize(
        switch_scan(
            page_change/2, page_event/2, machine/3, switch_scan_step, key([1])),
        _,
        SwitchScanProg),
    catch(
        program_plan(
            fixture(switch_scan_gap, SwitchScanProg, [], [], [])-[],
            _),
        SwitchScanError,
        true),
    SwitchScanError = unsupported_construct(edge_body_needs_pre(_)),
    format("PASS compiler names the one shared scan gap: edge_body_needs_pre~n").

current_surface_receipt :-
    parse_dl_file(
        'v6/prolog/compile/dl_view/ghcacher_host_program_term.dl6',
        HostProgram,
        _,
        []),
    HostProgram = program(HostDecls, HostRules, _),
    member(sh_decl(fetch, _, _, _), HostDecls),
    member((_ <- ProbeBody), HostRules),
    sub_term(probe(fetch, _, _, _), ProbeBody),
    prepare_program(HostProgram, ExpandedHostProgram, HostPlans, _, _),
    ExpandedHostProgram = prog(ExpandedDecls, _),
    member(host_plan(fetch, _, _, _, demand_ref('__host_demand_fetch'),
                     response_ref('__host_response_fetch')), HostPlans),
    member(keyed('__host_response_fetch'/_, _), ExpandedDecls),
    string_codes("rel f(a: int) -> (b: int).", FunctionRelCodes),
    catch(parse_dl(FunctionRelCodes, _, _, _), dl_parse_error(_, _), Refused = true),
    Refused == true,
    string_codes("rel child(id: int).\nuses(child).\n", RelArgCodes),
    parse_dl(RelArgCodes, prog(_, [(uses(child) <- true)]), _, []),
    format("PASS current surface: sh has input/output arrow; bind/rel do not; rel name in value position is an atom~n").
