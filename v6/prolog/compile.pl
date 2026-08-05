% Phase B entry point. Fixture terms are read with read_term/3 and
% variable_names/1 so source variable identity remains available to analysis.
%
% Emit:  swipl -q -l v6/prolog/compile/compile.pl \
%          -g "compile_fixture(switch_as_keyed_replace, 'v6/prolog/conformance/fixtures/scopes.pl', 'v6/prolog/compile/out/switch_as_keyed_replace.ts')" \
%          -g halt
%
:- module(compile,
          [ read_fixture_term/4,
            compile_fixture/3,
            compile_fixture/4,
            compile_dl6/2,
            compile_program/6,
            dl6_seeded_form/3,
            measure_phase/3,
            restore_phase_outcome/1,
            write_compile_trace/2,
            throw_text_door_error/2,
            program_plan/2
          ]).

:- use_module(library(lists)).
:- use_module('0_refusal_messages', []).
:- use_module('1_expansion',
              [ expand_program/3, expand_program_with_bindings/4 ]).
:- use_module('1_host_expand', [prepare_program/5]).
:- use_module(analyze).
:- use_module('3_clock_check', [check_clock_program/1]).
:- use_module('2_subscribe', [subscribed_rels/4]).
:- use_module(strat).
:- use_module(lower).
:- use_module(emit_ts).
:- use_module(library(tableutil), [table_statistics/2]).
:- use_module('compile/parse_dl',
              [ parse_dl_file/4,
                parse_dl_line_for_reason/2
              ]).
:- use_module('diag', [emit_diag_file/2]).
:- use_module('0_type_plane', [world_row_shape_violation/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- meta_predicate measure_phase(0, -, -).

% ═══ reading ═════════════════════════════════════════════════════════════════

read_fixture_term(File, Name, Term, Bindings) :-
    open(File, read, Stream),
    call_cleanup(find_fixture(Stream, Name, Term, Bindings), close(Stream)).

% Operators declared with op/3 inside a module are local to THAT module's own
% clause parsing; they do not carry over to a raw read_term/3 stream read.
% Every fixture file opens with its own `:- op(...)` directives (FIXTURES.md:
% "copy them from fixtures/merge_family.pl"), which normal consult would
% execute in sequence, making <-/<+/:= visible for the REST of that same
% file. This reader replays that: a directive term is CALLED, exactly what
% consult does, so the file's own operator declarations take effect for
% later terms in the same scan. compile.pl's own top-of-file op/3 lines are
% therefore redundant with this replay but kept for readability/IDE parsing.
find_fixture(Stream, Name, Term, Bindings) :-
    read_term(Stream, Candidate, [variable_names(CandidateBindings)]),
    ( Candidate == end_of_file
    -> throw(fixture_not_found(Name))
    ; Candidate = (:- Directive)
    -> call(Directive), find_fixture(Stream, Name, Term, Bindings)
    ; Candidate = fixture(Name, _, _, _, _)
    -> Term = Candidate, Bindings = CandidateBindings
    ; find_fixture(Stream, Name, Term, Bindings)
    ).

% ═══ the compile plan : everything lower.pl and emit_ts.pl need, computed
% once so both stay pure functions of it rather than re-deriving it ═════════
%
% plan(Name, Prog, RelPlans, ArrivalTargets, RuleOrder, EdgeRules,
%      SubscribedRels)
%   RelPlans: list of relplan(Ref, Kind, Columns, KeyPositionsOrNone,
%             ColumnTypes) covering every ref program_refs/2 or typed
%             declaration finds (arrival
%             targets and derived rels alike -- the tick log envelope
%             reports both). ColumnTypes (PHASE C2 RULING 1) is int|text per
%             Columns position, inferred by analyze.pl:rel_column_types/5
%             from declaration types when present, otherwise from the
%             fixture's own literal values -- lower.pl:column_def/3 is the
%             only SQL storage reader.
%   RuleOrder: level rules in strat.pl:sql_rule_order/2 order.
%   EdgeRules: edge rules, program order (engine.pl tries edge rules in
%              program order for each occurrence; with at most one edge rule
%              per target fixture this is a formality kept for generality).
%   SubscribedRels: 2_subscribe.pl's cone, sorted Name/Arity. Computed here
%              and threaded to emission; nothing else reads it.

% 1_host_expand.pl is SHARED with the reference engine, so its refusals are
% thrown in the oracle's vocabulary: a bare term. This door wraps them, the
% same way analyze.pl wraps every trigger it takes from 0_program_check.pl,
% because "every compiler refusal is unsupported_construct/1" is what the
% refusal-message umbrella and the sweep's supported/unsupported split both
% read -- scripts/text_door_receipt.pl:classify_term_door_error/3 treats
% anything else as a HARNESS failure, which is how the first host-refusal
% fixtures in the corpus surfaced this. Six host refusals predate them
% (refused_host_decl, column_mismatch, probe_mismatch, bind_mismatch,
% host_executor_mismatch, query_mismatch) and none had a fixture, so nothing
% had ever put one through this door.
%
% Wrapping HERE rather than inside the pre-pass keeps the pre-pass shared and
% keeps the ORACLE's terms bare, which is the same per-door split every
% trigger in 0_program_check.pl already has. Callers reaching
% compile_host_decl/2 directly still see the bare term; only whole-program
% compilation wraps.
prepare_program_for_compiler(SugaredProg, HostProg) :-
    catch(prepare_program(SugaredProg, HostProg, _, _, _), Refusal,
          throw_as_compiler_refusal(Refusal)).

throw_as_compiler_refusal(unsupported_construct(Reason)) :-
    !,
    throw(unsupported_construct(Reason)).
throw_as_compiler_refusal(Refusal) :-
    throw(unsupported_construct(Refusal)).

materialize_reference_target_rels(prog(Decls0, Rules), prog(Decls, Rules)) :-
    findall(col_type(Name/Arity, Column, Type),
            ( member(type_decl(Name, Specs), Decls0),
              length(Specs, Arity),
              member(col(Column, Type), Specs),
              \+ memberchk(col_type(Name/Arity, Column, Type), Decls0) ),
            MissingColumns),
    append(Decls0, MissingColumns, Decls).

program_plan(fixture(Name, SugaredProg, Initial, Schedule, _Expectations)-Bindings, Plan) :-
    prepare_program_for_compiler(SugaredProg, HostProg),
    % Host preparation stays a PRE-PASS (see engine.pl); the sugar phases run
    % in the order 1_expansion.pl declares.
    expand_program_with_bindings(HostProg, Bindings, ExpandedProg, _),
    materialize_reference_target_rels(ExpandedProg, Prog),
    Prog = prog(Decls, Rules),
    % ..._expanded/1, not check_supported_subset/1: Prog is ALREADY expanded
    % here, and the sugared entry expands again. That second expansion was the
    % redundant order site rank R3 removes.
    check_supported_subset_expanded(Prog),
    check_clock_program(Prog),
    % STRUCT-AS-ROWS, the compiler's half of SLOT-ARRIVAL-MALFORMED. The
    % oracle refuses the same rows in engine.pl:check_world_shapes/3; here it
    % runs at PLAN time so a malformed program never reaches emission and
    % lands in the sweep's `unsupported` bucket beside every other named
    % refusal, rather than compiling and then throwing mid-replay.
    check_world_shapes(Prog, Initial, Schedule),
    % Union rule-derived refs with EVERY declared ref (analyze.pl:
    % declared_refs/2's header comment) -- a kind(Ref, _) decl that no rule
    % ever mentions is still a real rel a schedule can write, and must still
    % get a table + arrival handling in the emitted program -- and with every
    % ref an Initial row seeds (analyze.pl:seeded_refs/2), which the oracle
    % stores whether or not anything declares it.
    program_refs(Rules, RuleRefs),
    declared_refs(Decls, DeclaredRefs),
    seeded_refs(Initial, SeededRefs),
    append([RuleRefs, DeclaredRefs, SeededRefs], AllRefs0), sort(AllRefs0, AllRefs),
    derived_refs(Rules, DerivedRefs),
    subtract(AllRefs, DerivedRefs, ArrivalTargets),
    % EXPRESSION + AGGREGATE LIFT: one program-wide typing fixpoint replaces
    % the per-ref rel_column_types/7 call. Same answer for every column that
    % has a literal witness or a declaration (so every pre-lift fixture keeps
    % its exact types); the difference is a column written ONLY by a level
    % rule's head expression, which now inherits that expression's type
    % instead of falling to the no-witness TEXT default. That default was the
    % TEXT-collapse ("12" vs 12) fail-first check (a) names.
    program_column_types(Decls, Rules, Initial, Schedule, Bindings, AllRefs, RefTypes),
    findall(relplan(Ref, Kind, Columns, KeyOrNone, ColumnTypes),
            ( member(Ref, AllRefs),
              rel_kind(Decls, Ref, Kind),
              rel_columns(Decls, Rules, Bindings, Ref, Columns),
              memberchk(Ref-ColumnTypes, RefTypes),
              ( decl_key(Decls, Ref, Positions) -> KeyOrNone = key(Positions) ; KeyOrNone = none )
            ), RelPlans),
    % PHASE C2 RULING 1 x RULING 2: this needs RelPlans (ColumnTypes), so it
    % runs here rather than inside check_supported_subset/1 above (which
    % runs before RelPlans exists).
    check_edge_head_column_types(RelPlans, Rules),
    sql_rule_order(Rules, RuleOrder),
    include(rule_is_edge, Rules, EdgeRules),
    % The query decls are the cone's only seeds, read from the POST-expansion
    % Decls for the same reason emit_ts.pl:world_plan_lines/2 reads them there.
    findall(QueryAtom, member(query(QueryAtom), Decls), Queries),
    subscribed_rels(Decls, Rules, Queries, SubscribedRels),
    Plan = plan(Name, Prog, RelPlans, ArrivalTargets, RuleOrder, EdgeRules,
                SubscribedRels).

% ═══ top level ═══════════════════════════════════════════════════════════════

compile_fixture(Name, FixtureFile, OutFile) :-
    compile_fixture(Name, FixtureFile, OutFile, emit_ts:emit_program).

compile_fixture(Name, FixtureFile, OutFile, Emitter) :-
    read_fixture_term(FixtureFile, Name, Term, Bindings),
    Term = fixture(Name, _Prog, Initial, _Schedule, _Expectations),
    compile_program(Name, Term, Bindings, Initial, OutFile, Emitter).

check_world_shapes(prog(Decls, _), Initial, Schedule) :-
    append([Initial | Schedule], WorldRows),
    (   world_row_shape_violation(Decls, WorldRows, mismatch(Ref, Column, TypeName, Reason))
    ->  ( Reason = int_out_of_range(Value)
        -> throw(unsupported_construct(int_out_of_range(Ref, Column, Value)))
        ;  throw(unsupported_construct(
                    type_arrival_shape_mismatch(Ref, Column, TypeName, Reason)))
        )
    ;   true
    ).

compile_dl6(File, OutFile) :-
    catch(
        ( run_compile_phase(parse,
                            parse_dl_file(File, Prog, Bindings, Findings),
                            ParseMeasurement),
          ( Findings == []
          -> true
          ; throw(unsupported_construct(surface_findings(Findings)))
          ) ),
        ParseError,
        ( emit_diag_file(File, ParseError),
          throw(ParseError) )
    ),
    file_base_name(File, BaseName),
    file_name_extension(Name, _Extension, BaseName),
    dl6_seeded_form(Prog, Initial, ProgOut),
    catch(
        compile_program_phases(Name, fixture(Name, ProgOut, Initial, [], []),
                               Bindings, Initial, OutFile, emit_ts:emit_program,
                               PhaseMeasurements),
        Error,
        throw_text_door_error(File, Error)
    ),
    write_compile_trace(
        Name, [phase(parse, ParseMeasurement) | PhaseMeasurements]).

% Shared by the two text-door callers that parse .dl6 themselves.
% Ground bodiless clauses become seed rows; non-ground ones stay refused.
dl6_seeded_form(Prog, Initial, ProgOut) :-
    Prog = prog(Decls, Rules),
    !,
    partition_dl6_facts(Rules, Initial, RealRules),
    ProgOut = prog(Decls, RealRules).
dl6_seeded_form(Prog, Initial, ProgOut) :-
    Prog = program(Decls, Rules, Queries),
    !,
    partition_dl6_facts(Rules, Initial, RealRules),
    ProgOut = program(Decls, RealRules, Queries).
dl6_seeded_form(Prog, [], Prog).

partition_dl6_facts([], [], []).
partition_dl6_facts([Rule | Rules], [Fact | Facts], Rest) :-
    dl6_fact(Rule, Fact),
    !,
    partition_dl6_facts(Rules, Facts, Rest).
partition_dl6_facts([Rule | Rules], Facts, [Rule | Rest]) :-
    partition_dl6_facts(Rules, Facts, Rest).

% Structured-term arguments (e.g. a ts_query value) are not seed-row shaped;
% they keep the rule path their fixtures already compile through.
dl6_fact((Head <- true), Head) :-
    !,
    ground(Head),
    fact_args_atomic(Head).
dl6_fact(Term, Term) :-
    ground(Term),
    \+ Term = match(_, _),
    fact_args_atomic(Term).

fact_args_atomic(Fact) :-
    Fact =.. [_ | Args],
    forall(member(Arg, Args), atomic(Arg)).

% EXPORTED because `bop check` is the SECOND caller of the text door and was
% getting an unlocated refusal for the identical file (cold-author defect D3):
% scripts/bop_check.pl calls compile_program/6 itself (it owns the temp output
% file and the exit-code mapping), so wrapping only at compile_dl6/2's own
% catch site left the CLI printing "rule-index unavailable" where the compile
% script printed `broken.dl6:4`. One wrapper, two catch sites; the line comes
% from parse_dl.pl's source_statement_fact/3, asserted by whichever
% parse_dl_file/4 call preceded the compile.
throw_text_door_error(File, Error) :-
    Error = unsupported_construct(at(_, _, _)),
    !,
    emit_diag_file(File, Error),
    throw(Error).
throw_text_door_error(File, unsupported_construct(Reason)) :-
    ( parse_dl_line_for_reason(Reason, Line)
    -> Emitted = unsupported_construct(at(File, Line, Reason))
    ;  Emitted = unsupported_construct(Reason)
    ),
    emit_diag_file(File, Emitted),
    throw(Emitted).
throw_text_door_error(_File, Error) :-
    throw(Error).

compile_program(Name, Term, Bindings, Initial, OutFile, Emitter) :-
    compile_program_phases(Name, Term, Bindings, Initial, OutFile, Emitter,
                           PhaseMeasurements),
    zero_phase_measurement(EmptyMeasurement),
    write_compile_trace(
        Name, [phase(parse, EmptyMeasurement) | PhaseMeasurements]).

compile_program_phases(Name, Term, Bindings, Initial, OutFile, Emitter,
                       PhaseMeasurements) :-
    run_compile_phase(plan,
                      program_plan(Term-Bindings, Plan),
                      PlanMeasurement),
    run_compile_phase(lower,
                      lower_program(Plan, Lowered),
                      LowerMeasurement),
    Plan = plan(_, prog(Decls, _), RelPlans, _, _, _, _),
    Lowered = lowered(_, _, _, _, LevelStatements, _, _, _),
    run_compile_phase(
        boot,
        boot_statements(Decls, RelPlans, Initial, LevelStatements,
                        BootStatements),
        BootMeasurement),
    run_compile_phase(
        emit,
        call(Emitter, Name, Plan, Lowered, BootStatements, Text),
        EmitMeasurement),
    run_compile_phase(
        write,
        write_compiled_output(OutFile, Text),
        WriteMeasurement),
    PhaseMeasurements = [ phase(plan, PlanMeasurement),
                          phase(lower, LowerMeasurement),
                          phase(boot, BootMeasurement),
                          phase(emit, EmitMeasurement),
                          phase(write, WriteMeasurement)
                        ].

write_compiled_output(OutFile, Text) :-
    setup_call_cleanup(
        open(OutFile, write, Stream),
        format(Stream, "~s", [Text]),
        close(Stream)),
    format("wrote ~w~n", [OutFile]).

run_compile_phase(_Phase, Goal, Measurement) :-
    measure_phase(Goal, Measurement, Outcome),
    restore_phase_outcome(Outcome).

measure_phase(Goal, Measurement, Outcome) :-
    setup_call_cleanup(
        statistics_snapshot(Before),
        phase_outcome(Goal, Outcome),
        capture_phase_measurement(Before, Measurement)).

phase_outcome(Goal, Outcome) :-
    catch(
        ( once(call(Goal))
        -> Outcome = ok
        ; Outcome = failed
        ),
        Error,
        Outcome = error(Error)).

restore_phase_outcome(ok).
restore_phase_outcome(failed) :-
    fail.
restore_phase_outcome(error(Error)) :-
    throw(Error).

capture_phase_measurement(Before, Measurement) :-
    statistics_snapshot(After),
    statistics_delta(Before, After,
                     WallMs, CpuMs, Inferences,
                     GcCount, GcReclaimedBytes, GcMs, GcLeftBytes,
                     TableCount, TableAnswers, TableReuses,
                     TableSpaceBytes, TableCompiledSpaceBytes),
    Measurement = measurement(
        WallMs, CpuMs, Inferences,
        GcCount, GcReclaimedBytes, GcMs, GcLeftBytes,
        TableCount, TableAnswers, TableReuses,
        TableSpaceBytes, TableCompiledSpaceBytes).

statistics_snapshot(
        stats(CpuSeconds, Inferences, WallMilliseconds,
              GcCount, GcReclaimedBytes, GcMilliseconds, GcLeftBytes,
              TableCount, TableAnswers, TableReuses,
              TableSpaceBytes, TableCompiledSpaceBytes)) :-
    statistics(cputime, CpuSeconds),
    statistics(inferences, Inferences),
    statistics(walltime, [WallMilliseconds, _SinceLast]),
    statistics(garbage_collection,
               [GcCount, GcReclaimedBytes, GcMilliseconds, GcLeftBytes]),
    table_statistics(tables, TableCount),
    table_statistics(answers, TableAnswers),
    table_statistics(complete_call, TableReuses),
    table_statistics(space, TableSpaceBytes),
    table_statistics(compiled_space, TableCompiledSpaceBytes).

statistics_delta(
        stats(Cpu0, Inf0, Wall0, GcCount0, GcBytes0, GcMs0, _GcLeft0,
              TableCount0, TableAnswers0, TableReuses0,
              TableSpace0, TableCompiledSpace0),
        stats(Cpu1, Inf1, Wall1, GcCount1, GcBytes1, GcMs1, GcLeft1,
              TableCount1, TableAnswers1, TableReuses1,
              TableSpace1, TableCompiledSpace1),
        WallMs, CpuMs, Inferences,
        GcCount, GcReclaimedBytes, GcMs, GcLeft1,
        TableCount, TableAnswers, TableReuses,
        TableSpaceBytes, TableCompiledSpaceBytes) :-
    round_two(Wall1 - Wall0, WallMs),
    round_two((Cpu1 - Cpu0) * 1000, CpuMs),
    Inferences is Inf1 - Inf0,
    GcCount is GcCount1 - GcCount0,
    GcReclaimedBytes is GcBytes1 - GcBytes0,
    GcMs is GcMs1 - GcMs0,
    TableCount is TableCount1 - TableCount0,
    TableAnswers is TableAnswers1 - TableAnswers0,
    TableReuses is TableReuses1 - TableReuses0,
    TableSpaceBytes is TableSpace1 - TableSpace0,
    TableCompiledSpaceBytes is TableCompiledSpace1 - TableCompiledSpace0.

round_two(Value, Rounded) :-
    Rounded is round(Value * 100) / 100.

zero_phase_measurement(
        measurement(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)).

write_compile_trace(Name, PhaseMeasurements) :-
    phase_trace_measurement(PhaseMeasurements, parse, ParseMeasurement),
    phase_trace_measurement(PhaseMeasurements, plan, PlanMeasurement),
    phase_trace_measurement(PhaseMeasurements, lower, LowerMeasurement),
    phase_trace_measurement(PhaseMeasurements, boot, BootMeasurement),
    phase_trace_measurement(PhaseMeasurements, emit, EmitMeasurement),
    phase_trace_measurement(PhaseMeasurements, write, WriteMeasurement),
    phase_trace_measurement_values(ParseMeasurement, ParseWall, ParseInf),
    phase_trace_measurement_values(PlanMeasurement, PlanWall, PlanInf),
    phase_trace_measurement_values(LowerMeasurement, LowerWall, LowerInf),
    phase_trace_measurement_values(BootMeasurement, BootWall, BootInf),
    phase_trace_measurement_values(EmitMeasurement, EmitWall, EmitInf),
    phase_trace_measurement_values(WriteMeasurement, WriteWall, WriteInf),
    TotalWall is ParseWall + PlanWall + LowerWall + BootWall + EmitWall + WriteWall,
    TotalInf is ParseInf + PlanInf + LowerInf + BootInf + EmitInf + WriteInf,
    format(user_error,
           "COMPILE-TRACE program=~w parse=~w/~w plan=~w/~w lower=~w/~w boot=~w/~w emit=~w/~w write=~w/~w total=~w/~w~n",
           [ Name,
             ParseWall, ParseInf,
             PlanWall, PlanInf,
             LowerWall, LowerInf,
             BootWall, BootInf,
             EmitWall, EmitInf,
             WriteWall, WriteInf,
             TotalWall, TotalInf
           ]).

phase_trace_measurement(PhaseMeasurements, Phase, Measurement) :-
    memberchk(phase(Phase, Measurement), PhaseMeasurements).

phase_trace_measurement_values(measurement(WallMs, _CpuMs, Inferences, _GcCount,
                                           _GcReclaimedBytes, _GcMs,
                                           _GcLeftBytes, _TableCount,
                                           _TableAnswers, _TableReuses,
                                           _TableSpaceBytes,
                                           _TableCompiledSpaceBytes),
                               WallMs, Inferences).
