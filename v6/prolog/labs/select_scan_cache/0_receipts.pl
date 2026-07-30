% 0_receipts.pl : select + scan + generic switch-cache lab.
%
% Production code is not edited by this lab. It specializes compile-time
% relation/rule names into the existing prog/2 IR, then runs the shared oracle
% and compiler.
%
% Run:
%   swipl -q -l v6/prolog/labs/select_scan_cache/0_receipts.pl -g go -g halt

:- module(select_scan_cache_receipts, [go/0]).

:- use_module('../../conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../compile/compile', [program_plan/2, compile_program/6]).
:- use_module('../../compile/lower', [lower_program/2]).
:- use_module('../../compile/parse_dl', [parse_dl/4]).
:- use_module(library(readutil), [read_file_to_string/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Compile-time call. Every argument is a named program relation or rule.
cache_call(
    make_switch_map_cache(
        outer(route_change/2),
        key_rule(route_key/2),
        scope(open_scope/2),
        response(route_response/2),
        decode_rule(decode_value/2),
        cache(route_cache/2),
        demand(route_demand/1),
        output(route_output/3),
        finalized(route_finalized/2),
        effect(route_effect/1))).

canonical_signature(
    sig(
        inputs([
            outer(route_change/2, log),
            key_rule(route_key/2, det, grade(0)),
            response(route_response/2, log, async(at_least(1))),
            decode_rule(decode_value/2, det, grade(0))
        ]),
        outputs([
            scope(open_scope/2, keyed([1])),
            cache(route_cache/2, keyed([1])),
            demand(route_demand/1, level),
            output(route_output/3, level),
            finalized(route_finalized/2, log),
            effect(route_effect/1, log)
        ]),
        clock([
            reduce(grade(0)),
            commit(boundary),
            listener(grade(1)),
            async_response(at_least(1))
        ]),
        cardinality([
            key_rule(exactly_one),
            decode_rule(exactly_one),
            scope(at_most_one_per_owner),
            cache(at_most_one_per_key)
        ]),
        lifetime(scope(until_next_outer_same_owner)),
        effects([route_effect/1]))).

% Tier N -> N-1. The generated rules contain no function/relation values.
specialize_cache(Call, Signature, Program) :-
    Call = make_switch_map_cache(
        outer(OuterName/2),
        key_rule(KeyRuleName/2),
        scope(ScopeName/2),
        response(ResponseName/2),
        decode_rule(DecodeRuleName/2),
        cache(CacheName/2),
        demand(DemandName/1),
        output(OutputName/3),
        finalized(FinalizedName/2),
        effect(EffectName/1)),
    OuterRef = OuterName/2,
    ScopeRef = ScopeName/2,
    ResponseRef = ResponseName/2,
    CacheRef = CacheName/2,
    FinalizedRef = FinalizedName/2,
    EffectRef = EffectName/1,
    canonical_signature(Signature),
    Decls = [
        kind(OuterRef, log), keep(OuterRef, all),
        kind(ResponseRef, log), keep(ResponseRef, all),
        keyed(ScopeRef, [1]),
        keyed(CacheRef, [1]),
        kind(FinalizedRef, log), keep(FinalizedRef, all),
        kind(EffectRef, log), keep(EffectRef, all)
    ],
    Outer =.. [OuterName, Owner, OuterValue],
    KeyRule =.. [KeyRuleName, OuterValue, CacheKey],
    Scope =.. [ScopeName, Owner, CacheKey],
    Response =.. [ResponseName, CacheKey, WireValue],
    DecodeRule =.. [DecodeRuleName, WireValue, Value],
    Cache =.. [CacheName, CacheKey, Value],
    Demand =.. [DemandName, CacheKey],
    Output =.. [OutputName, Owner, CacheKey, Value],
    Finalized =.. [FinalizedName, Owner, CacheKey],
    Effect =.. [EffectName, CacheKey],
    Rules = [
        (Scope <+ Outer, latest(KeyRule)),
        (Cache <+ Response, latest(DecodeRule)),
        (Demand <- Scope, not(Cache)),
        (Output <- Scope, Cache),
        (Finalized <+ finalize(Scope)),
        (Effect <+ Demand)
    ],
    Program = prog(Decls, Rules).

cache_initial([
    route_key(settings, settings_api),
    route_key(profile, profile_api),
    route_key(reports, reports_api),
    decode_value(raw_settings, settings_body),
    decode_value(raw_profile, profile_body),
    decode_value(raw_reports, reports_body),
    decode_value(raw_late_settings, late_settings_body)
]).

cache_program(Program) :-
    cache_call(Call),
    specialize_cache(Call, _, Program).

go :-
    cache_hit_receipt,
    cache_miss_and_delayed_effect_receipt,
    switch_late_fill_stale_rejection_receipt,
    multiple_keys_receipt,
    finalize_receipt,
    retry_pagination_arithmetic_receipt,
    source_order_receipt,
    rollback_receipt,
    match_value_and_side_write_receipt,
    ordinary_composition_vs_specialization_receipt,
    sql_erasure_receipt,
    clock_signature_receipt,
    format("12 PASS~n").

cache_hit_receipt :-
    cache_program(Program),
    cache_initial(Base),
    append(Base, [route_cache(settings_api, settings_body)], Initial),
    run_program(Program, Initial, [[+route_change(owner_a, settings)]],
                Final, Deltas),
    rel_rows(route_output/3, Final,
             [route_output(owner_a, settings_api, settings_body)]),
    rel_rows(route_demand/1, Final, []),
    rel_rows(route_effect/1, Final, []),
    rel_deltas(route_output/3, Deltas,
               [[+route_output(owner_a, settings_api, settings_body)], []]),
    format("PASS cache hit emits output with no demand or effect~n").

cache_miss_and_delayed_effect_receipt :-
    cache_program(Program),
    cache_initial(Initial),
    run_program(
        Program,
        Initial,
        [[+route_change(owner_a, profile)], [], [+route_response(profile_api, raw_profile)]],
        Final,
        Deltas),
    rel_rows(route_cache/2, Final,
             [route_cache(profile_api, profile_body)]),
    rel_rows(route_output/3, Final,
             [route_output(owner_a, profile_api, profile_body)]),
    rel_deltas(route_demand/1, Deltas,
               [[+route_demand(profile_api)], [],
                [-route_demand(profile_api)], []]),
    rel_deltas(route_effect/1, Deltas,
               [[], [+route_effect(profile_api)], [], []]),
    format("PASS cache miss commits demand; listener effect is delayed; async response fills later~n").

switch_late_fill_stale_rejection_receipt :-
    cache_program(Program),
    cache_initial(Initial),
    Schedule = [
        [+route_change(owner_a, settings)],
        [+route_change(owner_a, profile)],
        [+route_response(settings_api, raw_late_settings)],
        [+route_response(profile_api, raw_profile)]
    ],
    run_program(Program, Initial, Schedule, Final, Deltas),
    rel_rows(route_cache/2, Final,
             [route_cache(profile_api, profile_body),
              route_cache(settings_api, late_settings_body)]),
    rel_rows(route_output/3, Final,
             [route_output(owner_a, profile_api, profile_body)]),
    rel_deltas(route_output/3, Deltas,
               [[], [], [], [+route_output(owner_a, profile_api, profile_body)],
                []]),
    format("PASS switch permits late cache fill and rejects stale output~n").

multiple_keys_receipt :-
    cache_program(Program),
    cache_initial(Initial),
    Schedule = [
        [+route_change(owner_a, settings),
         +route_change(owner_b, profile)],
        [+route_response(settings_api, raw_settings),
         +route_response(profile_api, raw_profile)]
    ],
    run_program(Program, Initial, Schedule, Final, _),
    rel_rows(open_scope/2, Final,
             [open_scope(owner_a, settings_api),
              open_scope(owner_b, profile_api)]),
    rel_rows(route_output/3, Final,
             [route_output(owner_a, settings_api, settings_body),
              route_output(owner_b, profile_api, profile_body)]),
    format("PASS switch-cache partitions independent owners~n").

finalize_receipt :-
    cache_program(Program),
    cache_initial(Initial),
    run_program(
        Program,
        Initial,
        [[+route_change(owner_a, settings)],
         [+route_change(owner_a, profile)]],
        Final,
        Deltas),
    rel_rows(route_finalized/2, Final,
             [route_finalized(owner_a, settings_api)]),
    rel_deltas(route_finalized/2, Deltas,
               [[], [], [+route_finalized(owner_a, settings_api)], []]),
    format("PASS keyed switch produces one delayed finalize occurrence~n").

% One merged Log is the select queue. Event order is the outside-arrival list.
% The Seq column is a durable witness, not a request to sort an unordered Set.
scan_program(
    prog(
        [kind(select_event/3, log), keep(select_event/3, all),
         keyed(machine/4, [1])],
        [
            (machine(Key, PageNext, Retry, TotalNext) <+
                select_event(Key, _, page(Count)),
                pre(machine(Key, Page, Retry, Total)),
                PageNext := Page + 1,
                TotalNext := Total + Count),
            (machine(Key, Page, RetryNext, Total) <+
                select_event(Key, _, retry),
                pre(machine(Key, Page, Retry, Total)),
                RetryNext := Retry + 1),
            (machine(Key, Page, 0, Total) <+
                select_event(Key, _, success),
                pre(machine(Key, Page, _Retry, Total)))
        ])).

retry_pagination_arithmetic_receipt :-
    scan_program(Program),
    Initial = [machine(job_a, 0, 0, 0), machine(job_b, 10, 2, 100)],
    Schedule = [[
        +select_event(job_a, 1, page(5)),
        +select_event(job_b, 2, success),
        +select_event(job_a, 3, retry),
        +select_event(job_a, 4, page(7))
    ]],
    run_program(Program, Initial, Schedule, Final, Deltas),
    rel_rows(machine/4, Final,
             [machine(job_a, 2, 1, 12), machine(job_b, 10, 0, 100)]),
    rel_deltas(machine/4, Deltas,
               [[-machine(job_a, 0, 0, 0),
                 -machine(job_b, 10, 2, 100),
                 +machine(job_a, 2, 1, 12),
                 +machine(job_b, 10, 0, 100)],
                []]),
    format("PASS ordered select scan handles match arms and integer arithmetic~n").

source_order_receipt :-
    scan_program(Program),
    Initial = [machine(job_a, 0, 0, 0)],
    run_program(
        Program,
        Initial,
        [[+select_event(job_a, 1, retry),
          +select_event(job_a, 2, success)]],
        RetryThenSuccess,
        _),
    rel_rows(machine/4, RetryThenSuccess, [machine(job_a, 0, 0, 0)]),
    run_program(
        Program,
        Initial,
        [[+select_event(job_a, 1, success),
          +select_event(job_a, 2, retry)]],
        SuccessThenRetry,
        _),
    rel_rows(machine/4, SuccessThenRetry, [machine(job_a, 0, 1, 0)]),
    format("PASS merged Log arrival order is the deterministic select order~n").

rollback_receipt :-
    Program = prog(
        [kind(bad_event/1, log), keep(bad_event/1, all),
         keyed(bad_state/2, [1])],
        [(bad_state(Key, a) <+ bad_event(Key)),
         (bad_state(Key, b) <+ bad_event(Key))]),
    catch(
        run_program(Program, [bad_state(owner, seed)],
                    [[+bad_event(owner)]], _, _),
        Error,
        true),
    Error = keyed_conflict(
        bad_state/2,
        [owner],
        [bad_state(owner, a), bad_state(owner, b)]),
    format("PASS ambiguous reducer rows conflict before state application~n").

match_value_and_side_write_receipt :-
    string_codes(
        "rel input(value: int) log keep(all).\nrel result(value: int).\nrel side(value: int) log keep(all).\nmatch input(Value) (\n  ; Value >= 0 |-> result(Value)\n  ; Value < 0 |+> side(Value)\n).\n",
        Codes),
    parse_dl(Codes, prog(_, Rules), _, []),
    member(match(input(Value),
                 ((result(Value) <- Value >= 0) ;
                  (side(Value) <+ Value < 0))),
           Rules),
    format("PASS |-> is a pure relational result and |+> is an edge write~n").

ordinary_composition_vs_specialization_receipt :-
    cache_call(Call),
    specialize_cache(Call, _, Program),
    \+ higher_order_term(Program),
    Program = prog(_, Rules),
    length(Rules, 6),
    forall(member(Rule, Rules), ordinary_rule(Rule)),
    format("PASS specialization only chooses names; behavior is ordinary <-/<+ composition~n").

higher_order_term(Program) :-
    sub_term(Term, Program),
    nonvar(Term),
    functor(Term, Functor, _),
    memberchk(Functor,
              [make_switch_map_cache, outer, key_rule, scope, response,
               decode_rule, cache, demand, output, finalized, effect, sig]).

ordinary_rule((_ <- _)).
ordinary_rule((_ <+ _)).

sql_erasure_receipt :-
    cache_call(Call),
    specialize_cache(Call, _, Program),
    Fixture = fixture(select_scan_cache_sql, Program, [], [], []),
    program_plan(Fixture-[], Plan),
    lower_program(Plan, Lowered),
    Lowered = lowered(_, Ddl, _, EdgeStatements, _, _, _, _),
    member(DdlStatement, Ddl),
    sub_string(DdlStatement, _, _, _, "open_scope"),
    member(edgestmt(open_scope/2, route_change/2, _, _, _, _, _, _),
           EdgeStatements),
    tmp_file_stream(text, EmittedFile, Stream),
    close(Stream),
    setup_call_cleanup(
        true,
        (with_output_to(
             string(_),
             compile_program(
                 select_scan_cache_sql,
                 Fixture,
                 [],
                 [],
                 EmittedFile,
                 emit_ts:emit_program)),
         read_file_to_string(EmittedFile, Emitted, []),
         sub_string(Emitted, _, _, _, "open_scope"),
         \+ sub_string(Emitted, _, _, _, "makeSwitchMapCache"),
         \+ sub_string(Emitted, _, _, _, "make_switch_map_cache")),
        delete_file(EmittedFile)),
    format("PASS generic cache erases before fixed SQL and TypeScript emission~n").

clock_signature_receipt :-
    canonical_signature(Signature),
    Signature = sig(_, _, clock(Clock), _, _, _),
    memberchk(reduce(grade(0)), Clock),
    memberchk(commit(boundary), Clock),
    memberchk(listener(grade(1)), Clock),
    memberchk(async_response(at_least(1)), Clock),
    format("PASS canonical signature records reduce, commit, listener, and async grades~n").
