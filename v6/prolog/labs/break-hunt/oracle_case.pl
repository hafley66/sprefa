% oracle_case.pl -- break-hunt lab door: run the REFERENCE ENGINE over one
% .dl6 text program and a JSON arrival schedule, printing the shared tick-log
% envelope AND the final-state line.
%
% compile/scripts/dl6_oracle.pl prints the tick log only; compile/oracle_dump.pl
% writes the final line only for TERM-door fixtures. A case whose schedule is
% empty, or whose defect lives in the accumulated store rather than in a
% delta, needs both legs off the same .dl6 text.
%
% Run: swipl -q -l oracle_case.pl -g "oracle_case('p.dl6','s.json')" -g halt

:- ensure_loaded('../../conformance/ticklog').
:- use_module('../../next/0_parse/parse_dl_dcg', [parse_dl_file/4]).
:- use_module('../../compile/scripts/0_json_arrival', [arrival_column_types/4, schedule_value/5]).

oracle_case(Dl6File, ScheduleFile) :-
    parse_dl_file(Dl6File, Prog, Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "oracle_case: parse findings ~q~n", [Findings]), halt(1)
    ),
    read_schedule(Prog, Bindings, ScheduleFile, Schedule),
    print_ticklog(Prog, [], Schedule),
    run_program(Prog, [], Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, Line),
    format("~w~n", [Line]).

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
    maplist(schedule_value(oracle_case, Rel), ColumnTypes, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "oracle_case: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).

% Identical to compile/oracle_dump.pl's leg; that file is a script, not a
% module, so the clauses cannot be imported.
final_state_line(FinalAll, Line) :-
    findall(Ref-Row, ( member(Row, FinalAll), rel_ref(Row, Ref) ), Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Grouped),
    findall(RelJson, ( member(Ref-Rows, Grouped), final_rel_json(Ref, Rows, RelJson) ), RelJsons),
    atomic_list_concat(RelJsons, ',', Inner),
    format(atom(Line), '{"final":{~w}}', [Inner]).

final_rel_json(Name/_Arity, Rows, Json) :-
    maplist(row_json, Rows, RowJsonsRaw),
    msort(RowJsonsRaw, RowJsons),
    atomic_list_concat(RowJsons, ',', Inner),
    format(atom(Json), '"~w":[~w]', [Name, Inner]).
