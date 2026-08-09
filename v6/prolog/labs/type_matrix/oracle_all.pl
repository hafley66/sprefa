% oracle_all.pl : run the REFERENCE ENGINE over ONE generated cell, write its
% tick log plus final-state line, and print a one-line JSON verdict.
% drive.mjs invokes this once per cell.
%
% Both halves are borrowed, not rebuilt:
%
%   read_schedule/4        compile/scripts/dl6_oracle.pl -- the TYPE-DIRECTED
%                          arrival mapping (a json column's value arrives as
%                          text and is parsed; every other column keeps the
%                          integer/atom mapping). That type-direction is why
%                          this lab loads dl6_oracle.pl rather than
%                          golden_oracle.pl, whose mapping is structural and
%                          which a concurrent lane owns.
%   print_tick_lines/2     conformance/ticklog.pl, via oracle_dump.pl
%   final_state_line/2     compile/oracle_dump.pl
%
% ONE PROCESS PER CELL is FORCED here, not chosen: dl6_oracle.pl answers a
% malformed json-column arrival with `halt(1)`, which no in-process catch can
% survive -- a batched loop died at the first such cell with 419 ungraded. The
% driver reads a nonzero exit with no verdict line as `halt`, which is itself
% one of the matrix's findings rather than a harness detail.
%
% ── TWO ORACLE DOORS, because .dl6 text has two and they are not the same ───
%
% MEASURED, and it is one of the matrix's findings rather than a harness
% choice. The same program and the same schedule:
%
%   rel probe_in(value: float).   schedule [[{"rel":"probe_in","row":[1.5]}]]
%   dl6_oracle.pl     type_arrival_shape_mismatch(..., field_not_finite_float('1.5'))
%   golden_oracle.pl  {"tick":1,"deltas":{"probe_in":{"add":[[1.5]] ...
%
% dl6_oracle.pl's schedule_value/4 goes integer -> string -> term_to_atom, with
% no float and no boolean clause, so every float and every bool reaching a
% typed column arrives as an ATOM and the engine's own shape check refuses it.
% golden_oracle.pl HAS those clauses and lacks dl6_oracle's type-directed json
% mapping. Each door is missing what the other has, so this file runs BOTH and
% the classifier grades a cell against the door that accepted its arrival,
% naming the gap when they differ.
%
% Output: out/<id>.oracle.jsonl (dl6 door) and out/<id>.golden.jsonl (golden
% door); the verdict line carries both statuses.
%
% Run: swipl -q -l oracle_all.pl -g "one(cell_arrival__int__int)" -g halt

:- ensure_loaded('../../compile/oracle_dump').
:- ensure_loaded('../../compile/scripts/dl6_oracle').
:- ensure_loaded('../../compile/scripts/golden_oracle').
:- use_module('../../compile/parse_dl', [parse_dl_file/4]).
:- use_module(library(http/json)).

one(Id) :-
    atomic_list_concat(['out/', Id, '.dl6'], Source),
    atomic_list_concat(['out/', Id, '.schedule.json'], ScheduleFile),
    atomic_list_concat(['out/', Id, '.oracle.jsonl'], LogFile),
    atomic_list_concat(['out/', Id, '.golden.jsonl'], GoldenFile),
    door_result(dl6, Source, ScheduleFile, LogFile, Status, Detail),
    door_result(golden, Source, ScheduleFile, GoldenFile, GoldenStatus, GoldenDetail),
    print_result(Status, Detail, GoldenStatus, GoldenDetail).

door_result(Door, Source, ScheduleFile, LogFile, Status, Detail) :-
    (   catch(cell_log(Door, Source, ScheduleFile, Text), Error, true)
    ->  ( var(Error)
        -> Status = ok, Detail = "", write_log(LogFile, Text)
        ;  unsupported_detail(Error, Status, Detail) )
    ;   Status = error, Detail = "oracle run failed without throwing"
    ).

% The tick lines and the final line in ONE file, so a cell's whole graded
% surface is a single byte comparison later. `golden-run.ts --final` prints the
% same two parts in the same order on the emitter side.
cell_log(Door, Source, ScheduleFile, Text) :-
    parse_dl_file(Source, Prog, Bindings, Findings),
    (   Findings == []
    ->  true
    ;   throw(parse_findings(Findings))
    ),
    door_schedule(Door, Prog, Bindings, ScheduleFile, Schedule),
    run_program(Prog, [], Schedule, FinalAll, DeltaTicks),
    final_state_line(FinalAll, FinalLine),
    with_output_to(string(TickText), print_tick_lines(1, DeltaTicks)),
    format(string(Text), "~w~w~n", [TickText, FinalLine]).

% ── THE TWO DOORS HAVE CONVERGED (measured 2026-07-31, on this base) ────────
%
% When this lab was written the two doors held DIFFERENT arrival mappings and
% different arities: read_schedule/4 was dl6_oracle.pl's, read_schedule/2 was
% golden_oracle.pl's, and the arity difference is what let both files load into
% one process untouched. That is no longer true. golden_oracle.pl now carries
% read_schedule/4 with the SAME signature, and both files delegate the actual
% value mapping to the shared compile/scripts/0_json_arrival.pl
% (schedule_value/5), whose only per-door argument is a Context atom used
% exclusively in halt/1 diagnostic text. The mapping is therefore identical.
%
% Two consequences, both recorded rather than papered over:
%
%   1  read_schedule/2 no longer exists, so the golden leg answered every cell
%      with existence_error(procedure, read_schedule/2). It is called at the
%      shared arity here; the leg runs again and the door-agreement axis
%      becomes a measurement instead of an error column.
%   2  Loading both files into one process now CLASHES: swipl reports
%      "Redefined static procedure" for read_schedule/4, batch_terms/4 and
%      arrival_term/4, and golden_oracle.pl (loaded second) wins. Both legs
%      consequently run golden_oracle.pl's clauses, which differ from
%      dl6_oracle.pl's only in the Context atom passed to the shared mapping.
%      The clash is behaviourally inert for grading and loudly warned at load.
%
% The two legs are kept rather than collapsed because "the doors agree" is a
% RESULT this matrix should keep re-measuring, not an assumption to bake in:
% if either door grows its own mapping again, this axis is what notices.
door_schedule(dl6, Prog, Bindings, ScheduleFile, Schedule) :-
    read_schedule(Prog, Bindings, ScheduleFile, Schedule).
door_schedule(golden, Prog, Bindings, ScheduleFile, Schedule) :-
    read_schedule(Prog, Bindings, ScheduleFile, Schedule).

% Same rule compile_all.pl states and for the same measured reason: an ISO
% `error/2` term is a crash, any other thrown compound is a named answer whose
% functor is its name. engine.pl's `type_arrival_shape_mismatch/4` and
% body.pl's `json_capture_type_unknown/1` are named answers that carry no
% unsupported construct wrapper at all.
unsupported_detail(error(Formal, Context), error, Detail) :- !,
    term_string(error(Formal, Context), Detail).
unsupported_detail(Term, refused, Detail) :-
    compound(Term), !,
    term_string(Term, Detail).
unsupported_detail(Term, error, Detail) :-
    term_string(Term, Detail).

write_log(LogFile, Text) :-
    setup_call_cleanup(open(LogFile, write, Stream),
                       format(Stream, "~w", [Text]),
                       close(Stream)).

print_result(Status, Detail, GoldenStatus, GoldenDetail) :-
    with_output_to(string(Line),
                   json_write_dict(current_output,
                                   _{status: Status, detail: Detail,
                                     goldenStatus: GoldenStatus,
                                     goldenDetail: GoldenDetail},
                                   [width(0)])),
    format('~w~n', [Line]).
