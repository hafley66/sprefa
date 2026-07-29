% dl6_oracle.pl : run the REFERENCE ENGINE over a `.dl6` text program and a
% JSON arrival schedule, printing the shared tick-log envelope.
%
% Why this exists (runtime-bridge arc, plans/2026-07-29-runtime-bridge-header.md
% scope 4): the schedule-fed byte grading stays the referee for the SERVED
% engine too. oracle_dump.pl covers every conformance FIXTURE, whose program is
% a prolog term and whose schedule is a prolog list. A served program is `.dl6`
% TEXT and its schedule is the very JSON the http client posts. This file closes
% that gap by reading both in their served form and handing them to
% conformance/ticklog.pl's own `print_ticklog/3` -- the same predicate
% oracle_dump.pl calls, unedited, so the two logs are produced by one printer.
%
% The schedule file is the SAME file the http client posts from (the
% out/<name>.schedule.json shape sweep.pl already writes):
%
%   [ [ {"rel":"event","sign":"add","row":[1,"boot"]} ], ... ]
%
% VALUE MAPPING, stated because prolog makes it a real choice: a JSON number
% becomes an integer and a JSON string becomes an ATOM, not a prolog string.
% Atoms and strings both serialize as JSON strings on the way out (ticklog.pl
% `value_json/2`), so the printed log is identical either way -- but joins are
% not. The compiler's own generated text (a probe's witness digest, built by
% concatenation in 1_host_expand.pl) is an ATOM, so a schedule that fed a host
% response's witness column as a string would silently fail to join and the
% derived rows would vanish. Measured, not assumed: with strings, the served
% receipt (b) log carried `answered` at its third tick and the oracle's did not.
% The remaining edge, named rather than fixed: a program whose RULES carry
% double-quoted string literals compared against a world-fed column needs its
% schedule expressed as a fixture term instead.
%
% Run: swipl -q -l dl6_oracle.pl -g "oracle('p.dl6','s.json')" -g halt

:- ensure_loaded('../../conformance/ticklog').
:- use_module('../parse_dl', [parse_dl_file/4]).
:- use_module(library(http/json)).

oracle(Dl6File, ScheduleFile) :-
    parse_dl_file(Dl6File, Prog, _Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "dl6_oracle: parse findings ~q~n", [Findings]), halt(1)
    ),
    read_schedule(ScheduleFile, Schedule),
    print_ticklog(Prog, [], Schedule).

read_schedule(ScheduleFile, Schedule) :-
    setup_call_cleanup(
        open(ScheduleFile, read, Stream),
        json_read_dict(Stream, Batches, [value_string_as(string)]),
        close(Stream)),
    maplist(batch_terms, Batches, Schedule).

batch_terms(Batch, Terms) :-
    maplist(arrival_term, Batch, Terms).

arrival_term(Arrival, Term) :-
    atom_string(Rel, Arrival.rel),
    maplist(schedule_value, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "dl6_oracle: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).

schedule_value(Value, Value) :- integer(Value), !.
schedule_value(Value, Atom) :- string(Value), !, atom_string(Atom, Value).
schedule_value(Value, Atom) :- term_to_atom(Value, Atom).
