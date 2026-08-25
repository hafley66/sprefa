% dl6_oracle.pl : run the REFERENCE ENGINE over a `.dl6` text program and a
% JSON arrival schedule, printing the shared tick-log envelope.
%
% Run the reference engine over a .dl6 program and a JSON arrival schedule.
%
% The schedule file is the SAME file the http client posts from (the
% out/<name>.schedule.json shape sweep.pl already writes):
%
%   [ [ {"rel":"event","sign":"add","row":[1,"boot"]} ], ... ]
%
% JSON numbers become integers and JSON strings become atoms, preserving joins
% against text produced by host expansion.
% The remaining edge, named rather than fixed: a program whose RULES carry
% double-quoted string literals compared against a world-fed column needs its
% schedule expressed as a fixture term instead.
%
% ── the mapping is TYPE-DIRECTED at a `json` column ──────────────────────────
%
% The rule above is right for every SCALAR column and wrong for a json one,
% and it was wrong SILENTLY: a program declaring `rel event(payload: json)`
% and destructuring it with a brace pattern derived NOTHING through this door
% while its term-door twin derived every row. Both schedule spellings failed,
% for two different reasons:
%
%   "row": [ {"repo":"cli","stars":4} ]      the object fell to term_to_atom/2
%                                            and became the ATOM `#{repo:...}`,
%                                            a SWI dict's text, which no
%                                            pattern can ever match
%   "row": [ "{\"repo\":\"cli\"}" ]          the string became an atom and
%                                            STAYED an atom: json_decode/2
%                                            wants obj(Pairs), never text
%
% The shape a json column takes is not this file's choice to make -- the
% EMITTER door already fixed it, deliberately and in writing:
%
%   serve/4_http.ts:columnProblem   "`json` takes exactly what `text` takes,
%                                    and deliberately so ... A json document
%                                    arrives as its text."
%   compile/sweep.pl:arrival_value_json/4  writes a json column's schedule
%                                    entry as a JSON STRING carrying
%                                    canonical_json_text/2 of the value
%
% So a json column's schedule value is JSON TEXT, and this door PARSES it into
% the oracle's own json terms (obj(SortedPairs) / list / number / atom), which
% is exactly what the term-door fixtures feed. Non-json columns keep the
% mapping above, byte for byte.
%
% A non-scalar value at a json column is a NAMED REFUSAL rather than a
% structural map, and the unsupported construct is the point: accepting `{"repo":"cli"}` as
% a raw object here would make this door accept a schedule the served door
% answers 400 to, which is the divergence-by-unilateral-fix the json_flex
% verdict named (its Q2). Widening the arrival grammar to take a raw object is
% a real option, but it is ONE decision taken on BOTH doors at once, not a
% convenience added to the referee.
%
% Run: swipl -q -l dl6_oracle.pl -g "oracle('p.dl6','s.json')" -g halt

:- ensure_loaded('../../conformance/ticklog').
:- use_module('../../7_lower/use_resolve', [expand_uses/8]).
:- use_module('0_json_arrival', [arrival_column_types/4, schedule_value/5]).

oracle(Dl6File, ScheduleFile) :-
    expand_uses(Dl6File, [], [], _, Prog, _, Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "dl6_oracle: parse findings ~q~n", [Findings]), halt(1)
    ),
    read_schedule(Prog, Bindings, ScheduleFile, Schedule),
    print_ticklog(Prog, [], Schedule).

read_schedule(Prog, Bindings, ScheduleFile, Schedule) :-
    setup_call_cleanup(
        open(ScheduleFile, read, Stream),
        json_read_dict(Stream, Batches, [value_string_as(string)]),
        close(Stream)),
    maplist(batch_terms(Prog, Bindings), Batches, Schedule).

batch_terms(Prog, Bindings, Batch, Terms) :-
    maplist(arrival_term(Prog, Bindings), Batch, Terms).

arrival_term(Prog, Bindings, Arrival, Term) :-
    atom_string(Rel, Arrival.rel),
    length(Arrival.row, Arity),
    arrival_column_types(Prog, Bindings, Rel/Arity, ColumnTypes),
    maplist(schedule_value(dl6_oracle, Rel), ColumnTypes, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "dl6_oracle: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).
