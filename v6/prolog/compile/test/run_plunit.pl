% run_plunit.pl -- the entry the `just plunit` recipe runs.
%
% plunit_tests.pl declares the units; this file is the only thing that decides
% HOW they run. Two knobs, both env:
%
%   PLUNIT_JOBS      worker threads (default: cpu_count; 1 = the old
%                    sequential run, byte-for-byte the same test order inside
%                    each unit)
%   PLUNIT_SLOWEST   rows in the slowest-test and slowest-unit tables
%                    (default 15; 0 suppresses both tables)
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
    (   Failed + TimedOut =:= 0
    ->  halt(0)
    ;   halt(1)
    ).
