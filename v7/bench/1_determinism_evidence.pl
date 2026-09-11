% Bounded Prolog determinism evidence report.
%
% The default report captures one real fixture compile through temporary
% wrappers around an explicit predicate allowlist. Direct report helpers also
% accept recorded calls. No answer terms are serialized: solution counts stop
% at two, and call identities use normalized term hashes.

:- module(dl7_determinism_evidence,
          [ main/0,
            default_config/1,
            default_capture_config/1,
            compile_fixture_report/2,
            determinism_report/2,
            aggregate_captured_entries/2,
            classify_call/3,
            leftover_choicepoint/2
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(apply), [maplist/2, maplist/3]).
:- use_module(library(lists), [sum_list/2]).
:- use_module(library(pairs), [group_pairs_by_key/2]).
:- use_module(library(prolog_wrap),
              [ wrap_predicate/4,
                unwrap_predicate/2
              ]).
:- use_module(library(solution_sequences), []).
:- use_module(library(http/json), [json_write_dict/3]).
:- use_module('../src/1_libtime/0_evaluator',
              [ evaluate/4,
                integer_comparison/3,
                stratify_rules/3
              ]).
:- use_module('../src/2_comptime/1_checker', [check_datalog/4]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).

:- thread_local active_capture_selection/4.
:- thread_local captured_entry/8.

:- discontiguous report_table/1.

capture_wrapper_name(dl7_determinism_evidence_capture).
max_replay_inputs(32).

%% main is det.
%
% Usage:
%   swipl -q -s 1_determinism_evidence.pl -g main -t halt --
%   swipl -q -s 1_determinism_evidence.pl -g main -t halt -- --unknown-predicate
%   swipl -q -s 1_determinism_evidence.pl -g main -t halt -- --invalid-config
main :-
    current_prolog_flag(argv, Arguments),
    catch(
        (   report_from_arguments(Arguments, Report),
            report_json(Report, Json),
            json_write_dict(current_output, Json, [width(0)]),
            nl,
            report_table(Report)
        ),
        Error,
        ( format(user_error, 'DL7-DETERMINISM-ERROR ~q~n', [Error]),
          halt(2) )
    ).

report_from_arguments(Arguments, Report) :-
    (   Arguments == []
    ->  compile_fixture_report(
            'v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7', Report)
    ;   config_from_arguments(Arguments, Config),
        determinism_report(Config, Report)
    ).

config_from_arguments(Arguments, Config) :-
    (   Arguments == []
    ->  default_config(Config)
    ;   Arguments = ['--unknown-predicate' | _]
    ->  Config = evidence_config(
                    [selected(dl7_determinism_evidence,
                              missing_predicate, 0)],
                    [])
    ;   Arguments = ['--invalid-config' | _]
    ->  Config = invalid_config
    ;   Arguments = ['--help' | _]
    ->  format(user_error,
               'usage: 1_determinism_evidence.pl [--unknown-predicate|--invalid-config]~n',
               []),
        halt(0)
    ;   Config = invalid_config
    ).

%% default_config(-Config) is det.
default_config(evidence_config(Selected, Calls)) :-
    EmptyBasement = basement_program(
                        root_graph([], []),
                        datalog_program([], [], [])),
    Selected = [
        selected(dl7_evaluator, integer_comparison, 3),
        selected(dl7_evaluator, stratify_rules, 3),
        selected(dl7_evaluator, evaluate, 4),
        selected(dl7_checker, check_datalog, 4)
    ],
    Calls = [
        recorded(multiple, dl7_evaluator:integer_comparison(_Name, _Positive,
                                                             _Negative)),
        recorded(zero, dl7_evaluator:integer_comparison(no_such, _, _)),
        recorded(stratification, dl7_evaluator:stratify_rules([], _Strata,
                                                                _Diagnostics)),
        recorded(evaluation, dl7_evaluator:evaluate([], [], _Closure,
                                                    _EvaluationDiagnostics)),
        recorded(checker, dl7_checker:check_datalog(
                             EmptyBasement, [], _Checked, _CheckDiagnostics))
    ].

%% default_capture_config(-Config) is det.
default_capture_config(capture_config(Selected, MaxReplay)) :-
    max_replay_inputs(MaxReplay),
    Selected = [
        selected(dl7_evaluator, integer_comparison, 3, pure),
        selected(dl7_evaluator, stratify_rules, 3, pure),
        selected(dl7_evaluator, evaluate, 4, non_replayable),
        selected(dl7_checker, check_datalog, 4, non_replayable)
    ].

%% compile_fixture_report(+Fixture, -Report) is det.
compile_fixture_report(Fixture, Report) :-
    once(compile_fixture_report_body(Fixture, Report)).

compile_fixture_report_body(Fixture, Report) :-
    default_capture_config(Config),
    validate_capture_config(Config, Selected, MaxReplay),
    setup_call_cleanup(
        true,
        (   begin_capture(Selected),
            run_captured_compile(Fixture, CompileStatus)
        ),
        end_capture(Selected, CapturedEntries)),
    captured_report(Fixture, CompileStatus, Selected, MaxReplay,
                    CapturedEntries, Report).

run_captured_compile(Fixture, Status) :-
    (   compile_dl7(Fixture, _Rows, _Runtime, _Diagnostics)
    ->  Status = success
    ;   Status = failure
    ).

begin_capture(Selected) :-
    retractall(active_capture_selection(_, _, _, _)),
    retractall(captured_entry(_, _, _, _, _, _, _, _)),
    forall(member(selected(Module, Name, Arity, Replayability), Selected),
           ( assertz(active_capture_selection(Module, Name, Arity,
                                               Replayability)),
             install_capture_wrapper(Module, Name, Arity) )).

install_capture_wrapper(Module, Name, Arity) :-
    functor(Head, Name, Arity),
    Head =.. [_ | Arguments],
    BodyArguments = [Wrapped, Module, Name, Arity, Arguments],
    Body0 =.. [capture_entry | BodyArguments],
    Body = dl7_determinism_evidence:Body0,
    capture_wrapper_name(WrapperName),
    wrap_predicate(Module:Head, WrapperName, Wrapped, Body).

end_capture(Selected, CapturedEntries) :-
    findall(captured_entry(Module, Name, Arity, Mode, Fingerprint,
                           Replayability, CanonicalCall, CallCopy),
            captured_entry(Module, Name, Arity, Mode, Fingerprint,
                           Replayability, CanonicalCall, CallCopy),
            CapturedEntries),
    remove_capture_wrappers(Selected),
    retractall(active_capture_selection(_, _, _, _)),
    retractall(captured_entry(_, _, _, _, _, _, _, _)).

remove_capture_wrappers(Selected) :-
    capture_wrapper_name(WrapperName),
    forall(member(selected(Module, Name, Arity, _), Selected),
           catch(unwrap_predicate(Module:Name/Arity, WrapperName),
                 _, true)).

capture_entry(Wrapped, Module, Name, Arity, Arguments) :-
    active_capture_selection(Module, Name, Arity, Replayability),
    Call =.. [Name | Arguments],
    QualifiedCall = Module:Call,
    copy_term(QualifiedCall, CallCopy),
    canonical_call(QualifiedCall, CanonicalCall),
    groundness_mode(QualifiedCall, Mode),
    term_hash(CanonicalCall, Fingerprint),
    assertz(captured_entry(Module, Name, Arity, Mode, Fingerprint,
                           Replayability, CanonicalCall, CallCopy)),
    invoke_wrapped(Wrapped, Arguments).

invoke_wrapped(call(Closure), Arguments) :-
    functor(Closure, ClosureName, Arity),
    length(Arguments, Arity),
    ClosureCall =.. [ClosureName | Arguments],
    call(ClosureCall).

validate_capture_config(capture_config(Selected, MaxReplay),
                        Selected, MaxReplay) :-
    must_be(list, Selected),
    integer(MaxReplay),
    MaxReplay > 0,
    MaxReplay =< 256,
    bounded_length(Selected, 16, capture_predicates),
    maplist(validate_capture_predicate, Selected).

validate_capture_predicate(selected(Module, Name, Arity, Replayability)) :-
    (   selected_module_predicate(Module, Name, Arity),
        memberchk(Replayability, [pure, non_replayable])
    ->  true
    ;   throw(error(domain_error(capture_predicate,
                                 selected(Module, Name, Arity,
                                          Replayability)), _))
    ).

selected_module_predicate(Module, Name, Arity) :-
    atom(Module),
    atom(Name),
    integer(Arity),
    between(0, 64, Arity),
    current_predicate(Module:Name/Arity).

captured_report(Fixture, CompileStatus, _Selected, MaxReplay,
                CapturedEntries,
                fixture_report(Fixture, CompileStatus, ObservedCalls,
                               UniqueInputs, ReplayedInputs, Classifications)) :-
    aggregate_captured_entries(CapturedEntries, ObservedCalls),
    length(ObservedCalls, UniqueInputs),
    replay_captured_entries(ObservedCalls, MaxReplay,
                            Classifications, ReplayedInputs).

aggregate_captured_entries(CapturedEntries, ObservedCalls) :-
    maplist(captured_entry_pair, CapturedEntries, Pairs),
    keysort(Pairs, SortedPairs),
    group_pairs_by_key(SortedPairs, Groups),
    maplist(aggregate_captured_group, Groups, ObservedCalls).

captured_entry_pair(
    captured_entry(Module, Name, Arity, Mode, Fingerprint,
                   Replayability, CanonicalCall, CallCopy),
    key(predicate(Module, Name, Arity), Mode, Replayability,
        CanonicalCall)-captured_sample(Fingerprint, CallCopy)).

aggregate_captured_group(
    key(Predicate, Mode, Replayability, _CanonicalCall)-Samples,
    observed_call(Predicate, Mode, Fingerprint, Replayability, Count,
                  SampleCall)) :-
    length(Samples, Count),
    Samples = [captured_sample(Fingerprint, SampleCall) | _].

replay_captured_entries(ObservedCalls, MaxReplay,
                        Classifications, ReplayedInputs) :-
    replay_captured_entries(
        ObservedCalls, 0, MaxReplay, Classifications, ReplayedInputs).

replay_captured_entries([], _, _, [], 0).
replay_captured_entries(
    [ObservedCall | ObservedCalls], Index0, MaxReplay,
    [Classification | Classifications], ReplayedInputs) :-
    replay_one_observed_call(
        ObservedCall, Index0, MaxReplay, Classification, Index, Replayed),
    replay_captured_entries(
        ObservedCalls, Index, MaxReplay, Classifications, RestReplayed),
    ReplayedInputs is Replayed + RestReplayed.

replay_one_observed_call(
    observed_call(Predicate, Mode, Fingerprint, non_replayable, Count, _),
    Index, _,
    classification(Predicate, Mode, Fingerprint, Count,
                   non_replayable, none, none),
    Index, 0).
replay_one_observed_call(
    observed_call(Predicate, Mode, Fingerprint, pure, Count, Call),
    Index0, MaxReplay,
    Classification, Index, Replayed) :-
    (   Index0 < MaxReplay
    ->  classify_call(Call, ReplayMode, SolutionClass),
        leftover_choicepoint(Call, LeavesChoicepoint),
        Classification = classification(
                             Predicate, ReplayMode, Fingerprint, Count,
                             replayed, SolutionClass, LeavesChoicepoint),
        Index is Index0 + 1,
        Replayed = 1
    ;   Classification = classification(
                             Predicate, Mode, Fingerprint, Count,
                             replay_capped, capped, none),
        Index = Index0,
        Replayed = 0
    ).

%% determinism_report(+Config, -Report) is det.
determinism_report(Config, Report) :-
    once(determinism_report_body(Config, Report)).

determinism_report_body(Config, report(Observations)) :-
    validate_config(Config, Selected, Calls),
    maplist(observe_record(Selected), Calls, RawObservations),
    aggregate_observations(RawObservations, Observations).

%% classify_call(+QualifiedCall, -GroundnessMode, -SolutionClass) is det.
classify_call(QualifiedCall, GroundnessMode, SolutionClass) :-
    must_be(callable, QualifiedCall),
    groundness_mode(QualifiedCall, GroundnessMode),
    once(classify_call_body(QualifiedCall, SolutionClass)).

classify_call_body(QualifiedCall, SolutionClass) :-
    findnsols(2, marker, call(QualifiedCall), Markers),
    solution_class(Markers, SolutionClass).

%% leftover_choicepoint(+QualifiedCall, -LeavesChoicepoint) is det.
%
% The first successful answer is observed before committing. The cleanup flag
% is bound immediately for a deterministic call and remains unbound while the
% called predicate retains an alternative. `once/1` surrounds only the probe
% after the flag has been read, so the evidence is itself deterministic.
leftover_choicepoint(QualifiedCall, LeavesChoicepoint) :-
    must_be(callable, QualifiedCall),
    once(leftover_choicepoint_probe(QualifiedCall, LeavesChoicepoint)).

leftover_choicepoint_probe(QualifiedCall, LeavesChoicepoint) :-
    findnsols(1, marker, call(QualifiedCall), Markers),
    (   Markers == []
    ->  LeavesChoicepoint = none
    ;   once(( call_cleanup(call(QualifiedCall), CleanupFlag = true),
               choicepoint_flag(CleanupFlag, LeavesChoicepoint) ))
    ).

choicepoint_flag(CleanupFlag, LeavesChoicepoint) :-
    (   nonvar(CleanupFlag)
    ->  LeavesChoicepoint = false
    ;   LeavesChoicepoint = true
    ).

observe_record(Selected, recorded(_Label, QualifiedCall), Observation) :-
    call_identity(QualifiedCall, Predicate, Fingerprint),
    selected_predicate(Selected, QualifiedCall),
    classify_call(QualifiedCall, GroundnessMode, SolutionClass),
    leftover_choicepoint(QualifiedCall, LeavesChoicepoint),
    Observation = observation(Predicate, GroundnessMode, Fingerprint,
                              SolutionClass, LeavesChoicepoint, 1).

validate_config(Config, Selected, Calls) :-
    (   Config = evidence_config(Selected0, Calls0)
    ->  Selected = Selected0,
        Calls = Calls0,
        must_be(list, Selected),
        must_be(list, Calls),
        bounded_length(Selected, 16, selected_predicates),
        bounded_length(Calls, 32, recorded_calls),
        maplist(validate_selected_predicate, Selected),
        maplist(validate_recorded_call(Selected), Calls)
    ;   throw(error(domain_error(determinism_evidence_config, invalid), _))
    ).

validate_selected_predicate(Selected) :-
    (   Selected = selected(Module, Name, Arity),
        atom(Module),
        atom(Name),
        integer(Arity),
        between(0, 64, Arity)
    ->  (   current_predicate(Module:Name/Arity)
        ->  true
        ;   throw(error(existence_error(predicate,
                                        Module:Name/Arity), _))
        )
    ;   throw(error(domain_error(selected_predicate, Selected), _))
    ).

validate_recorded_call(Selected, RecordedCall) :-
    (   RecordedCall = recorded(Label, QualifiedCall),
        atom(Label),
        callable(QualifiedCall),
        selected_predicate(Selected, QualifiedCall)
    ->  true
    ;   throw(error(domain_error(recorded_call, RecordedCall), _))
    ).

selected_predicate(Selected, QualifiedCall) :-
    call_identity(QualifiedCall, Predicate, _),
    Predicate = predicate(Module, Name, Arity),
    memberchk(selected(Module, Name, Arity), Selected).

call_identity(QualifiedCall, predicate(Module, Name, Arity), Fingerprint) :-
    strip_module(QualifiedCall, Module, PlainCall),
    callable(PlainCall),
    functor(PlainCall, Name, Arity),
    normalized_fingerprint(QualifiedCall, Fingerprint).

normalized_fingerprint(Term, Fingerprint) :-
    canonical_call(Term, CanonicalCall),
    term_hash(CanonicalCall, Fingerprint).

canonical_call(Term, CanonicalCall) :-
    copy_term(Term, CanonicalCall),
    numbervars(CanonicalCall, 0, _).

groundness_mode(QualifiedCall, GroundnessMode) :-
    strip_module(QualifiedCall, _, PlainCall),
    PlainCall =.. [_Functor | Arguments],
    maplist(argument_groundness, Arguments, Modes),
    atomic_list_concat(Modes, '-', GroundnessMode).

argument_groundness(Argument, Mode) :-
    (   ground(Argument)
    ->  Mode = ground
    ;   Mode = variable
    ).

solution_class([], zero).
solution_class([_], one).
solution_class([_, _], multiple).

bounded_length(List, Limit, Label) :-
    length(List, Length),
    (   Length =< Limit
    ->  true
    ;   throw(error(domain_error(bounded_list, Label-Length), _))
    ).

aggregate_observations(RawObservations, Observations) :-
    maplist(observation_pair, RawObservations, Pairs),
    keysort(Pairs, SortedPairs),
    group_pairs_by_key(SortedPairs, Groups),
    maplist(aggregate_observation_group, Groups, Observations).

observation_pair(
    observation(Predicate, GroundnessMode, Fingerprint,
                SolutionClass, LeavesChoicepoint, Count),
    key(Predicate, GroundnessMode, Fingerprint,
        SolutionClass, LeavesChoicepoint)-Count).

aggregate_observation_group(
    key(Predicate, GroundnessMode, Fingerprint,
        SolutionClass, LeavesChoicepoint)-Counts,
    observation(Predicate, GroundnessMode, Fingerprint,
                SolutionClass, LeavesChoicepoint, CallCount)) :-
    sum_list(Counts, CallCount).

report_json(report(Observations),
            _{schema: "dl7-determinism-evidence-v1",
              observations: JsonObservations}) :-
    maplist(observation_json, Observations, JsonObservations).

report_json(
    fixture_report(Fixture, CompileStatus, ObservedCalls,
                   UniqueInputs, ReplayedInputs, Classifications),
    _{classification: JsonClassifications,
      compile_status: CompileStatus,
      fixture: Fixture,
      observed_calls: JsonObservedCalls,
      replayed_inputs: ReplayedInputs,
      schema: "dl7-determinism-evidence-v1",
      unique_inputs: UniqueInputs}) :-
    maplist(observed_call_json, ObservedCalls, JsonObservedCalls),
    maplist(classification_json, Classifications, JsonClassifications).

observation_json(
    observation(predicate(Module, Name, Arity), GroundnessMode, Fingerprint,
                SolutionClass, LeavesChoicepoint, CallCount),
    _{call_count: CallCount,
      fingerprint: Fingerprint,
      groundness: GroundnessMode,
      leaves_choicepoint: LeavesChoicepoint,
      predicate: PredicateText,
      solution_class: SolutionClass}) :-
    format(atom(PredicateText), '~w:~w/~d', [Module, Name, Arity]).

observed_call_json(
    observed_call(predicate(Module, Name, Arity), GroundnessMode, Fingerprint,
                  Replayability, CallCount, _),
    _{call_count: CallCount,
      fingerprint: Fingerprint,
      groundness: GroundnessMode,
      predicate: PredicateText,
      replayability: Replayability}) :-
    format(atom(PredicateText), '~w:~w/~d', [Module, Name, Arity]).

classification_json(
    classification(predicate(Module, Name, Arity), GroundnessMode, Fingerprint,
                   CallCount, Status, SolutionClass, LeavesChoicepoint),
    _{call_count: CallCount,
      fingerprint: Fingerprint,
      groundness: GroundnessMode,
      leaves_choicepoint: LeavesChoicepoint,
      predicate: PredicateText,
      solution_class: SolutionClass,
      status: Status}) :-
    format(atom(PredicateText), '~w:~w/~d', [Module, Name, Arity]).

report_table(report(Observations)) :-
    format(user_error,
           'DL7-DETERMINISM predicate | groundness | calls | solutions | leaves_choicepoint | fingerprint~n',
           []),
    forall(member(Observation, Observations), report_table_row(Observation)).

report_table_row(
    observation(predicate(Module, Name, Arity), GroundnessMode, Fingerprint,
                SolutionClass, LeavesChoicepoint, CallCount)) :-
    format(user_error, '~w:~w/~d | ~w | ~d | ~w | ~w | ~d~n',
           [Module, Name, Arity, GroundnessMode, CallCount, SolutionClass,
            LeavesChoicepoint, Fingerprint]).

report_table(
    fixture_report(Fixture, CompileStatus, ObservedCalls,
                   UniqueInputs, ReplayedInputs, Classifications)) :-
    length(ObservedCalls, ObservedCount),
    format(user_error,
           'DL7-DETERMINISM fixture=~w status=~w observed_calls=~d unique_inputs=~d replayed_inputs=~d~n',
           [Fixture, CompileStatus, ObservedCount, UniqueInputs,
            ReplayedInputs]),
    format(user_error,
           'DL7-DETERMINISM predicate | groundness | calls | replayability | fingerprint~n',
           []),
    forall(member(ObservedCall, ObservedCalls),
           report_observed_call_row(ObservedCall)),
    format(user_error,
           'DL7-DETERMINISM classification predicate | status | solutions | leaves_choicepoint~n',
           []),
    forall(member(Classification, Classifications),
           report_classification_row(Classification)).

report_observed_call_row(
    observed_call(predicate(Module, Name, Arity), GroundnessMode, Fingerprint,
                  Replayability, CallCount, _)) :-
    format(user_error, '~w:~w/~d | ~w | ~d | ~w | ~d~n',
           [Module, Name, Arity, GroundnessMode, CallCount, Replayability,
            Fingerprint]).

report_classification_row(
    classification(predicate(Module, Name, Arity), _, _, _, Status,
                   SolutionClass, LeavesChoicepoint)) :-
    format(user_error, '~w:~w/~d | ~w | ~w | ~w~n',
           [Module, Name, Arity, Status, SolutionClass, LeavesChoicepoint]).
