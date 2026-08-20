% run_plunit.pl -- the entry the `just plunit` recipe runs.
%
% plunit_tests.pl declares the units; this file is the only thing that decides
% HOW they run. Knobs, all env:
%
%   PLUNIT_JOBS      worker threads (default: cpu_count; 1 = the old
%                    sequential run, byte-for-byte the same test order inside
%                    each unit)
%   PLUNIT_SLOWEST   rows in the slowest-test and slowest-unit tables
%                    (default 15; 0 suppresses both tables)
%   PLUNIT_JSON      path to write one JSON document; unset, nothing is
%                    written and stdout is unchanged.
%   PLUNIT_TAP       path to write a TAP version 13 stream, or `-` for
%                    stdout; unset, nothing is written.
%   PLUNIT_JUNIT     path to write junit-schema XML, converted from the same
%                    TAP stream through the `tap-junit` npm package; unset,
%                    nothing is written and stdout is byte-identical to
%                    before any of these knobs existed.
%
% plunit's jobs(N) schedules one UNIT per worker: tests inside a unit stay
% sequential and in file order, units interleave. Two units that touch the same
% global dynamic predicate therefore race, which is why parse_dl_dcg.pl's four
% parse-scratch facts are thread_local.
%
% cleanup(false) keeps plunit's passed/5 and failed/5 rows alive past the run so
% the timing table can be read off them; plunit's own `cleanup` runs at the
% START of the next run_tests/2, so nothing leaks between runs.

:- use_module(library(plunit)).
:- use_module(library(lists)).
:- use_module(library(pairs)).
:- use_module(library(aggregate)).
:- use_module(library(http/json)).
:- use_module(library(process)).

:- ensure_loaded('plunit_tests.pl').

% Captured at load time so the tap-junit binary resolves regardless of the
% caller's working directory (`just plunit` cd's into v6/prolog/compile).
:- prolog_load_context(directory, HereDir),
   asserta(plunit_here_dir(HereDir)).

plunit_jobs_count(Jobs) :-
    (   getenv('PLUNIT_JOBS', Text),
        atom_number(Text, Requested),
        integer(Requested)
    ->  true
    ;   current_prolog_flag(cpu_count, Requested)
    ),
    Jobs is max(1, Requested).

plunit_slowest_rows(Rows) :-
    (   getenv('PLUNIT_SLOWEST', Text),
        atom_number(Text, Requested),
        integer(Requested)
    ->  true
    ;   Requested = 15
    ),
    Rows is max(0, Requested).

% Every run_tests/2 outcome carries a wall time; a test that failed by throwing
% before its body started still has one, so no result kind is dropped here.
plunit_test_wall(Unit, Name, Wall, Status) :-
    plunit:passed(Unit, Name, _Line, _Det, Time),
    Status = passed,
    Wall = Time.wall.
plunit_test_wall(Unit, Name, Wall, Status) :-
    plunit:failed(Unit, Name, _Line, _Reason, Time),
    Status = failed,
    Wall = Time.wall.

plunit_failing_names(Names) :-
    findall(Unit:Name, plunit:failed(Unit, Name, _, _, _), Failed),
    findall(Unit:Name, plunit:timeout(Unit, Name, _, _, _), TimedOut),
    append(Failed, TimedOut, All),
    sort(All, Names).

% FAIL lines are the failing-SET receipt: one canonical `unit:name` per line,
% sorted, so two runs diff cleanly. plunit's own error report elides long names.
plunit_report_failures :-
    plunit_failing_names(Names),
    forall(member(Unit:Name, Names),
           format("FAIL ~w:~w~n", [Unit, Name])).

plunit_report_slowest_tests(0) :- !.
plunit_report_slowest_tests(Rows) :-
    findall(Wall-(Unit:Name/Status),
            plunit_test_wall(Unit, Name, Wall, Status),
            Pairs),
    sort(1, @>=, Pairs, Sorted),
    length(Sorted, Total),
    Take is min(Rows, Total),
    length(Head, Take),
    append(Head, _, Sorted),
    format("~n── slowest ~d tests of ~d ──~n", [Take, Total]),
    forall(member(Wall-(Unit:Name/Status), Head),
           format("SLOW ~3f ~w ~w:~w~n", [Wall, Status, Unit, Name])).

plunit_report_slowest_units(0) :- !.
plunit_report_slowest_units(Rows) :-
    findall(Unit-Wall,
            plunit_test_wall(Unit, _Name, Wall, _Status),
            UnitWalls),
    keysort(UnitWalls, Grouped),
    group_pairs_by_key(Grouped, Groups),
    findall(Total-Unit,
            ( member(Unit-Walls, Groups), sum_list(Walls, Total) ),
            Totals),
    sort(1, @>=, Totals, Sorted),
    length(Sorted, UnitCount),
    Take is min(Rows, UnitCount),
    length(Head, Take),
    append(Head, _, Sorted),
    format("~n── slowest ~d units of ~d ──~n", [Take, UnitCount]),
    forall(member(Total-Unit, Head),
           format("UNIT ~3f ~w~n", [Total, Unit])).

plunit_run :-
    plunit_jobs_count(Jobs),
    plunit_slowest_rows(Rows),
    get_time(Start),
    run_tests(all, [ jobs(Jobs), cleanup(false), summary(Summary) ]),
    get_time(End),
    Wall is End - Start,
    plunit_report_slowest_tests(Rows),
    plunit_report_slowest_units(Rows),
    format("~n"),
    plunit_report_failures,
    _{ total:Declared, passed:Passed, failed:Failed, timeout:TimedOut } :< Summary,
    aggregate_all(count, plunit_test_wall(_, _, _, _), Results),
    % `declared` is plunit's static test_count; `results` counts outcomes, and a
    % forall(...) test contributes one row per generated case, so results >= declared.
    format("PLUNIT jobs=~d declared=~d results=~d passed=~d failed=~d timeout=~d wall=~2fs~n",
           [Jobs, Declared, Results, Passed, Failed, TimedOut, Wall]),
    RunInfo = run_info(Jobs, Declared, Results, Passed, Failed, TimedOut, Wall, Start),
    plunit_reports_maybe_write(RunInfo),
    (   Failed + TimedOut =:= 0
    ->  halt(0)
    ;   halt(1)
    ).

                 /*******************************
                 *      SHARED CASE TABLE       *
                 *******************************/
%
% One row per generated outcome; a forall(...) test contributes one row per
% case under the same Unit:Name. JSON, TAP and (through TAP) JUNIT all read
% the same case(Unit, Name, Line, Time, Status, Detail) list built here, so
% there is exactly one place that folds plunit's raw passed/5, failed/5,
% timeout/5 facts into one row per DECLARED test.

plunit_case_row(Unit, Name, Line, Wall, passed, none) :-
    plunit:passed(Unit, Name, Line, _Det, Time),
    Wall = Time.wall.
plunit_case_row(Unit, Name, Line, Wall, failed, E) :-
    plunit:failed(Unit, Name, Line, E, Time),
    Wall = Time.wall.
plunit_case_row(Unit, Name, Line, Wall, timeout, timeout(Limit)) :-
    plunit:timeout(Unit, Name, Line, Limit, Time),
    Wall = Time.wall.

% Fold every row for one Unit:Name into one case: failed/timeout beats passed,
% time is the sum of all generated cases' wall times, failure text comes from
% the first non-passing row.
plunit_case(Unit-Name, case(Unit, Name, Line, Time, Status, Detail)) :-
    findall(L-W-S-D, plunit_case_row(Unit, Name, L, W, S, D), Rows),
    Rows = [L0-_-_-_|_],
    Line = L0,
    aggregate_all(sum(W), member(_-W-_-_, Rows), Time),
    (   member(_-_-failed-D0, Rows)
    ->  Status = failed, Detail = D0
    ;   member(_-_-timeout-D0, Rows)
    ->  Status = timeout, Detail = D0
    ;   Status = passed, Detail = none
    ).

% Sorted by Unit-Name: one deterministic order shared by the JSON tests array
% and the TAP case numbering, so a JSON/TAP diff of the same run lines up.
plunit_cases(Cases) :-
    findall(Unit-Name, plunit_case_row(Unit, Name, _, _, _, _), Pairs0),
    sort(Pairs0, Pairs),
    findall(Case, ( member(Key, Pairs), plunit_case(Key, Case) ), Cases).

% plunit's own passed/5, failed/5, timeout/5 facts carry no file, only the
% clause's source Line; the file is exposed separately by plunit:unit_file/2.
plunit_case_file(Unit, File) :-
    (   catch(plunit:unit_file(Unit, Abs), _, fail)
    ->  working_directory(Cwd, Cwd),
        (   relative_file_name(Abs, Cwd, Rel)
        ->  File = Rel
        ;   File = Abs
        )
    ;   File = null
    ).

% Failure text rendered through plunit's own failure//1 grammar (the same one
% plunit's terminal report uses) rather than re-deriving a message from the
% raw failure term, so reported text matches what a human already reads on a
% red run.
plunit_failure_text(none, "") :- !.
plunit_failure_text(timeout(Limit), Text) :-
    !,
    format(string(Text), "test exceeded its ~w second time limit", [Limit]).
plunit_failure_text(E, Text) :-
    phrase(plunit:failure(E), Tokens),
    with_output_to(string(Text), print_message_lines(current_output, "", Tokens)).

% Collapse a possibly multi-line rendered failure to one line, for contexts
% (TAP YAML message, XML attribute values) that cannot carry raw newlines.
plunit_failure_oneline(Text, OneLine) :-
    split_string(Text, "\n", "", Lines),
    exclude(==(""), Lines, NonEmpty),
    (   NonEmpty == []
    ->  OneLine = ""
    ;   atomics_to_string(NonEmpty, " | ", OneLine)
    ).

plunit_reports_maybe_write(RunInfo) :-
    plunit_cases(Cases),
    plunit_json_maybe_write(RunInfo, Cases),
    plunit_tap_string(Cases, TapText),
    plunit_tap_maybe_write(TapText),
    plunit_junit_maybe_write(TapText).

                 /*******************************
                 *             JSON             *
                 *******************************/
%
% PLUNIT_JSON unset: none of this runs, stdout is unchanged.
%
% One document: a `run` object (the same counters the PLUNIT summary line
% prints) and a `tests` array, one flat dict per DECLARED test -- no nesting
% inside a test entry -- so `jq` and SQLite's `json_each` read it without a
% schema. `null` (bare atom) is library(json)'s dict-mode JSON null, not
% `@(null)` (that spelling is for the non-dict json(Pairs) term reader).

plunit_json_path(Path) :-
    getenv('PLUNIT_JSON', Path0),
    Path0 \== '',
    !,
    Path = Path0.

plunit_json_maybe_write(RunInfo, Cases) :-
    (   plunit_json_path(Path)
    ->  plunit_json_write(Path, RunInfo, Cases)
    ;   true
    ).

plunit_json_run_dict(run_info(Jobs, Declared, Results, Passed, Failed, TimedOut, Wall, Start),
                     _{ jobs: Jobs, declared: Declared, results: Results,
                        passed: Passed, failed: Failed, timeout: TimedOut,
                        wall_seconds: Wall, started_ts: Start }).

plunit_json_test(case(Unit, Name, Line, Time, Status, Detail),
                 _{ unit: Unit, name: Name, status: Status,
                    time_seconds: Time, file: File, line: Line,
                    failure: Failure }) :-
    plunit_case_file(Unit, File),
    plunit_json_failure(Status, Detail, Failure).

plunit_json_failure(passed, _, null) :- !.
plunit_json_failure(_, Detail, Text) :-
    plunit_failure_text(Detail, Text).

plunit_json_write(Path, RunInfo, Cases) :-
    plunit_json_run_dict(RunInfo, RunDict),
    maplist(plunit_json_test, Cases, Tests),
    Doc = _{ run: RunDict, tests: Tests },
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        json_write_dict(Stream, Doc, [width(78), step(2), tab(200)]),
        close(Stream)).

                 /*******************************
                 *             TAP              *
                 *******************************/
%
% TAP version 13 (https://testanything.org/tap-version-13-specification.html):
% a plan line, one `ok`/`not ok` per DECLARED test in plunit_cases/1's order,
% failure detail as an indented YAML block on the `not ok` rows. This is also
% the source JUNIT converts from below, so it is always built, whether or not
% PLUNIT_TAP itself is set.

plunit_tap_string(Cases, TapText) :-
    with_output_to(string(TapText), plunit_tap_write_stream(current_output, Cases)).

plunit_tap_write_stream(Stream, Cases) :-
    length(Cases, N),
    format(Stream, "TAP version 13~n", []),
    format(Stream, "1..~d~n", [N]),
    plunit_tap_write_cases(Stream, Cases, 1).

plunit_tap_write_cases(_Stream, [], _N).
plunit_tap_write_cases(Stream, [Case|Rest], N) :-
    plunit_tap_write_case(Stream, Case, N),
    N1 is N + 1,
    plunit_tap_write_cases(Stream, Rest, N1).

plunit_tap_write_case(Stream, case(Unit, Name, _Line, _Time, passed, _Detail), N) :-
    !,
    format(Stream, "ok ~d - ~w:~w~n", [N, Unit, Name]).
plunit_tap_write_case(Stream, case(Unit, Name, _Line, _Time, Status, Detail), N) :-
    format(Stream, "not ok ~d - ~w:~w~n", [N, Unit, Name]),
    plunit_tap_write_yaml(Stream, Status, Detail).

plunit_tap_write_yaml(Stream, Status, Detail) :-
    plunit_failure_text(Detail, Text),
    plunit_failure_oneline(Text, OneLine),
    plunit_yaml_dquote(OneLine, Quoted),
    format(Stream, "  ---~n", []),
    format(Stream, "  message: ~w~n", [Quoted]),
    format(Stream, "  severity: fail~n", []),
    format(Stream, "  status: ~w~n", [Status]),
    format(Stream, "  ...~n", []).

% YAML double-quoted scalar escaping: backslash first, then the quote, so the
% backslash introduced by quote-escaping is never itself re-escaped.
plunit_yaml_dquote(Text, Quoted) :-
    split_string(Text, "\\", "", Parts1),
    atomics_to_string(Parts1, "\\\\", Text1),
    split_string(Text1, "\"", "", Parts2),
    atomics_to_string(Parts2, "\\\"", Text2),
    format(string(Quoted), "\"~s\"", [Text2]).

plunit_tap_path(Path) :-
    getenv('PLUNIT_TAP', Path0),
    Path0 \== '',
    !,
    Path = Path0.

plunit_tap_maybe_write(TapText) :-
    (   plunit_tap_path(Path)
    ->  plunit_tap_write(Path, TapText)
    ;   true
    ).

plunit_tap_write('-', TapText) :-
    !,
    write(user_output, TapText).
plunit_tap_write(Path, TapText) :-
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        write(Stream, TapText),
        close(Stream)).

                 /*******************************
                 *          JUNIT XML           *
                 *******************************/
%
% PLUNIT_JUNIT unset: none of this runs, stdout is unchanged.
%
% BUILD-VS-BUY: an earlier pass here hand-wrote the junit XML; that was the
% wrong call (`pack_list(junit)` alone is a thin check -- no SWI pack renders
% junit AND no SWI pack replaces TAP, but TAP itself is a real, tiny,
% already-built target: this file only has to emit it). JUNIT XML is bought:
% the TAP stream above is piped through the `tap-junit` npm package
% (v6/tsv2/package.json devDependency, so `pnpm install` pins it offline;
% no bare `npx` fetch at test time). `tap-junit` was picked over `tap-xunit`
% because `tap-xunit` prefixes every testcase name with its own `#N `
% ordinal (`#1 acyclic_guard:...`), mangling the `unit:name` spelling the
% JSON/TAP names carry; `tap-junit` keeps the TAP test description verbatim
% as the testcase name. Measured with a 3-case, 1-failure sample TAP file,
% both converters piped from the same input.

plunit_tap_junit_bin(Bin) :-
    plunit_here_dir(Dir),
    atomic_list_concat([Dir, '/../../../tsv2/node_modules/.bin/tap-junit'], Bin0),
    absolute_file_name(Bin0, Bin, [access(execute)]).

plunit_junit_path(Path) :-
    getenv('PLUNIT_JUNIT', Path0),
    Path0 \== '',
    !,
    Path = Path0.

plunit_junit_maybe_write(TapText) :-
    (   plunit_junit_path(Path)
    ->  plunit_junit_write(Path, TapText)
    ;   true
    ).

plunit_junit_write(Path, TapText) :-
    plunit_tap_junit_bin(Bin),
    process_create(Bin, [],
                    [ stdin(pipe(In)), stdout(pipe(Out)), stderr(pipe(Err)),
                      process(PID) ]),
    thread_create((write(In, TapText), close(In)), Writer, []),
    read_string(Out, _, XmlText),
    read_string(Err, _, ErrText),
    thread_join(Writer, _),
    process_wait(PID, exit(Code)),
    close(Out),
    close(Err),
    (   Code =:= 0
    ->  true
    ;   throw(error(plunit_junit_convert_failed(Code, ErrText), _))
    ),
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        write(Stream, XmlText),
        close(Stream)).
