% run_plunit.pl -- the entry the `just plunit` recipe runs.
%
% plunit_tests.pl declares the units; this file is the only thing that decides
% HOW they run. Three knobs, all env:
%
%   PLUNIT_JOBS      worker threads (default: cpu_count; 1 = the old
%                    sequential run, byte-for-byte the same test order inside
%                    each unit)
%   PLUNIT_SLOWEST   rows in the slowest-test and slowest-unit tables
%                    (default 15; 0 suppresses both tables)
%   PLUNIT_JUNIT     path to write a junit-schema XML report; unset, no XML
%                    is written and stdout is byte-identical to before this
%                    knob existed.
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
:- use_module(library(sgml)).

:- ensure_loaded('plunit_tests.pl').

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
    plunit_junit_maybe_write,
    (   Failed + TimedOut =:= 0
    ->  halt(0)
    ;   halt(1)
    ).

                 /*******************************
                 *          JUNIT XML          *
                 *******************************/
%
% PLUNIT_JUNIT unset: none of this runs, stdout is unchanged.
%
% BUILD-VS-BUY: no SWI pack emits junit (`pack_list('junit')` against the
% default server returns nothing at time of writing) and the base install
% carries no junit writer; library(sgml) supplies only generic XML
% attribute/cdata quoting (xml_quote_attribute/2, xml_quote_cdata/2), reused
% below instead of hand-escaping. The fixed 3-level junit schema
% (testsuites > testsuite > testcase) is small enough that a bespoke writer
% beats pulling in a general XML-tree builder for it.
%
% Failure text is rendered through plunit's own failure//1 grammar (the same
% one plunit's terminal report uses) rather than re-deriving a message from
% the raw failure term, so the XML text matches what a human already reads
% on a red run.

plunit_junit_path(Path) :-
    getenv('PLUNIT_JUNIT', Path0),
    Path0 \== '',
    !,
    Path = Path0.

plunit_junit_maybe_write :-
    (   plunit_junit_path(Path)
    ->  plunit_junit_write(Path)
    ;   true
    ).

% One row per generated outcome; a forall(...) test contributes one row per
% case under the same Unit:Name, folded into a single testcase below.
plunit_junit_row(Unit, Name, Line, Wall, passed, none) :-
    plunit:passed(Unit, Name, Line, _Det, Time),
    Wall = Time.wall.
plunit_junit_row(Unit, Name, Line, Wall, failed, E) :-
    plunit:failed(Unit, Name, Line, E, Time),
    Wall = Time.wall.
plunit_junit_row(Unit, Name, Line, Wall, timeout, timeout(Limit)) :-
    plunit:timeout(Unit, Name, Line, Limit, Time),
    Wall = Time.wall.

% Fold every row for one Unit:Name into one testcase: failed/timeout beats
% passed, time is the sum of all generated cases' wall times, failure text
% comes from the first non-passing row.
plunit_junit_case(Unit-Name, case(Unit, Name, Line, Time, Status, Detail)) :-
    findall(L-W-S-D, plunit_junit_row(Unit, Name, L, W, S, D), Rows),
    Rows = [L0-_-_-_|_],
    Line = L0,
    aggregate_all(sum(W), member(_-W-_-_, Rows), Time),
    (   member(_-_-failed-D0, Rows)
    ->  Status = failed, Detail = D0
    ;   member(_-_-timeout-D0, Rows)
    ->  Status = timeout, Detail = D0
    ;   Status = passed, Detail = none
    ).

plunit_junit_cases(Cases) :-
    findall(Unit-Name, plunit_junit_row(Unit, Name, _, _, _, _), Pairs0),
    sort(Pairs0, Pairs),
    findall(Case, ( member(Key, Pairs), plunit_junit_case(Key, Case) ), Cases).

plunit_junit_group_by_unit(Cases, ByUnit) :-
    findall(Unit-Case, ( member(Case, Cases), Case = case(Unit,_,_,_,_,_) ), Pairs),
    keysort(Pairs, Sorted),
    group_pairs_by_key(Sorted, ByUnit).

% Collapse a possibly multi-line rendered failure to one line for the
% attribute value; the CDATA body below carries the full text.
plunit_junit_oneline(Text, OneLine) :-
    split_string(Text, "\n", "", Lines),
    exclude(==(""), Lines, NonEmpty),
    (   NonEmpty == []
    ->  OneLine = ""
    ;   atomics_to_string(NonEmpty, " | ", OneLine)
    ).

plunit_junit_failure_text(none, "") :- !.
plunit_junit_failure_text(timeout(Limit), Text) :-
    !,
    format(string(Text), "test exceeded its ~w second time limit", [Limit]).
plunit_junit_failure_text(E, Text) :-
    phrase(plunit:failure(E), Tokens),
    with_output_to(string(Text), print_message_lines(current_output, "", Tokens)).

plunit_junit_write(Path) :-
    plunit_junit_cases(Cases),
    plunit_junit_group_by_unit(Cases, ByUnit),
    setup_call_cleanup(
        open(Path, write, Stream, [encoding(utf8)]),
        plunit_junit_write_stream(Stream, ByUnit),
        close(Stream)).

plunit_junit_write_stream(Stream, ByUnit) :-
    format(Stream, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>~n", []),
    format(Stream, "<testsuites>~n", []),
    forall(member(Unit-Cases, ByUnit),
           plunit_junit_write_suite(Stream, Unit, Cases)),
    format(Stream, "</testsuites>~n", []).

plunit_junit_write_suite(Stream, Unit, Cases) :-
    length(Cases, Total),
    aggregate_all(count, member(case(_,_,_,_,failed,_), Cases), Failures),
    aggregate_all(count, member(case(_,_,_,_,timeout,_), Cases), Timeouts),
    aggregate_all(sum(T), member(case(_,_,_,T,_,_), Cases), SuiteTime),
    xml_quote_attribute(Unit, QUnit),
    format(Stream,
           "  <testsuite name=\"~w\" tests=\"~d\" failures=\"~d\" errors=\"~d\" time=\"~3f\">~n",
           [QUnit, Total, Failures, Timeouts, SuiteTime]),
    forall(member(Case, Cases), plunit_junit_write_case(Stream, Case)),
    format(Stream, "  </testsuite>~n", []).

plunit_junit_write_case(Stream, case(Unit, Name, _Line, Time, Status, Detail)) :-
    xml_quote_attribute(Unit, QUnit),
    xml_quote_attribute(Name, QName),
    format(Stream, "    <testcase classname=\"~w\" name=\"~w\" time=\"~3f\">~n",
           [QUnit, QName, Time]),
    plunit_junit_write_body(Stream, Status, Detail),
    format(Stream, "    </testcase>~n", []).

plunit_junit_write_body(_Stream, passed, _) :- !.
plunit_junit_write_body(Stream, timeout, Detail) :-
    !,
    plunit_junit_failure_text(Detail, Text),
    plunit_junit_oneline(Text, OneLine),
    xml_quote_attribute(OneLine, QAttr),
    xml_quote_cdata(Text, QCdata),
    format(Stream, "      <failure message=\"~w\" type=\"timeout\">~w</failure>~n",
           [QAttr, QCdata]).
plunit_junit_write_body(Stream, failed, Detail) :-
    plunit_junit_failure_text(Detail, Text),
    plunit_junit_oneline(Text, OneLine),
    xml_quote_attribute(OneLine, QAttr),
    xml_quote_cdata(Text, QCdata),
    format(Stream, "      <failure message=\"~w\" type=\"failed\">~w</failure>~n",
           [QAttr, QCdata]).
