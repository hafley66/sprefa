% Phase C oracle side. Run each fixture through ticklog.pl and write the
% captured JSONL output, reporting engine rejection paths as ORACLE_THROW.
%
% No `:- module(...)` header, matching ticklog.pl/go.pl themselves: this
% file has to call fixture/5, print_ticklog/3, and run_program/5 unqualified,
% which only works loaded into the same (user) context ticklog.pl loads into.
%
% Run: swipl -q -l v6/prolog/compile/oracle_dump.pl -g dump_all -g halt

:- ensure_loaded('../conformance/ticklog').

:- dynamic(oracle_dump_dir_fact/1).
:- prolog_load_context(directory, Here), atomic_list_concat([Here, '/out'], OutDir),
   assertz(oracle_dump_dir_fact(OutDir)).

dump_all :-
    oracle_dump_dir_fact(OutDir),
    ( exists_directory(OutDir) -> true ; make_directory(OutDir) ),
    findall(Name, fixture(Name, _, _, _, _), Names),
    forall(member(Name, Names), dump_one(OutDir, Name)).

dump_one(OutDir, Name) :-
    catch(
        ( fixture(Name, Prog, Initial, Schedule, _Expectations),
          with_output_to(string(Text), print_ticklog(Prog, Initial, Schedule)),
          format(atom(OutFile), '~w/~w.oracle.jsonl', [OutDir, Name]),
          setup_call_cleanup(open(OutFile, write, Stream), format(Stream, "~w", [Text]), close(Stream)),
          dump_final_state(OutDir, Name, Prog, Initial, Schedule),
          format("ORACLE_OK ~w~n", [Name])
        ),
        Error,
        format("ORACLE_THROW ~w ~q~n", [Name, Error])
    ).

% ═══ final-state leg (EXPRESSION + AGGREGATE LIFT arc) ══════════════════════
% The tick log alone cannot grade a fixture whose Schedule is EMPTY: both
% sides print zero lines and the sweep calls that IDENTICAL. SCOREBOARD.md
% already flags four such vacuous passes; the expression and aggregate
% buckets this arc lifts are almost entirely empty-schedule fixtures (25 of
% the 30 target fixtures), so a tick-log-only grade would have claimed them
% on no evidence at all.
%
% This writes the SAME envelope shape the tick log uses, over run_program/5's
% FinalAll (store rows + the final level closure, engine.pl:run_ticks/7's own
% first clause) instead of over one tick's deltas:
%
%   {"final":{"relName":[[..],[..]],...}}
%
% Rel names ascending, rows sorted lexicographically by their own JSON text
% (the same msort/2 rule rel_delta_json/3 applies to add/del), a rel with
% zero rows omitted, values encoded by ticklog.pl's own value_json/2 (reused,
% never reimplemented: integers as JSON numbers, atoms and canonical compound
% text as JSON strings). Log-rel duplicate rows survive as duplicate entries,
% since FinalAll is a multiset (engine.pl store_rows/2 msorts without
% dedup).
dump_final_state(OutDir, Name, Prog, Initial, Schedule) :-
    run_program(Prog, Initial, Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, Line),
    format(atom(FinalFile), '~w/~w.oracle.final.jsonl', [OutDir, Name]),
    setup_call_cleanup(open(FinalFile, write, Stream),
                       format(Stream, "~w~n", [Line]),
                       close(Stream)).

% keysort/2 on Ref-Row puts the rel names in the same ascending order sort/2 on
% the bare refs did, in one pass over FinalAll instead of one pass per rel.
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
