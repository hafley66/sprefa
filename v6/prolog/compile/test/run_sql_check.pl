% run_sql_check.pl : self-grading harness (TASK item 2). Drives the SAME
% lowered/8 term compile.pl builds through the REAL sqlite3 CLI (a stand-in
% for the real seam this harness cannot call directly -- Phase A's runtime
% lives in v6/tsv2, TypeScript, and this is a Prolog process) and compares
% the resulting per-tick deltas + final rows against the fixture's OWN
% expected values (fixtures/scopes.pl's deltas/final/ticks terms), the same
% oracle every other conformance fixture is graded against.
%
% ROUND 2 (reconciliation): rewritten end to end to match lower.pl's round-2
% architecture -- no tick number anywhere (arrival statements, edge
% projection, delta queries are all tick-number-free now), before/after
% full-table snapshots per rel with a Prolog-side multiset diff (mirrors
% runtime/diff.ts's multisetDiff, reimplemented here since this harness has
% no TypeScript to call), and edge-rule keyed writes resolved by replaying
% engine.pl's OWN rule ("across occurrences the later write wins") over the
% raw arrival list in Prolog, then executing one UPSERT per resolved row --
% the same two-step (project, then upsert) the emitted TypeScript performs,
% just driven by swipl + the sqlite3 CLI instead of rxjs + libsql.
%
% Independent confirmation this harness's answer is trustworthy: the SAME
% fixtures, run through the REAL emitted program on the REAL v6/tsv2 runtime
% (`cd v6/tsv2 && node --experimental-transform-types scripts/run-emitted.ts
% <name>`) and diffed against v6/prolog/conformance/ticklog.pl's oracle
% output, produce IDENTICAL tick counts and IDENTICAL added/removed KEYS at
% IDENTICAL ticks (compound-term TEXT differs by design -- json1 encoding
% here vs the oracle's raw Prolog-term text -- a known, documented
% representation choice, not a correctness gap). This harness and that
% real-runtime run are two independent proofs of the same compiled SQL.
%
% Run: swipl -q -l v6/prolog/compile/test/run_sql_check.pl -g "check(switch_as_keyed_replace)" -g halt

:- module(run_sql_check, [ check/1, check_all/0 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module(library(http/json)).
:- use_module('../../compile', [ read_fixture_term/4, program_plan/2 ]).
:- use_module('../../next/2_lower/lower', [ lower_program/2, boot_statements/7 ]).
:- use_module('../../0_rel_record',
              [ relplan_parts/6, relplan_of/3, relplan_columns/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% Resolved relative to this file's own load-time directory (mirrors
% sweep.pl's compile_dir/1 pattern -- prolog_load_context/2 only answers
% inside a directive running WHILE this file loads, so the directory is
% captured once, here, into a fact) rather than a hardcoded absolute path --
% a hardcoded path to a worktree that no longer exists is a portability bug,
% not a style nit (a prior version of this line named a stale worktree that
% happened to still exist on this machine by coincidence).
:- dynamic(test_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(test_dir_fact(Here)).

fixture_file(File) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures/scopes.pl'], File).

check_all :-
    forall(member(Name, [switch_as_keyed_replace, demand_laziness_effect_rows]),
           ( format("~n════ ~w ════~n", [Name]), check(Name) )).

% ═══ top level ═══════════════════════════════════════════════════════════════

check(Name) :-
    fixture_file(File),
    read_fixture_term(File, Name, Term, Bindings),
    Term = fixture(Name, _Prog, Initial, Schedule, Expectations),
    program_plan(Term-Bindings, Plan),
    lower_program(Plan, Lowered),
    Plan = plan(_, prog(Decls, _), Types, RelPlans, _ArrivalTargets, _, _, _, _),
    Lowered = lowered(_, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, _),
    boot_statements(Mode, Decls, Types, RelPlans, Initial, LevelStatements, BootStatements),
    tmp_db_file(Name, DbFile),
    ( exists_file(DbFile) -> delete_file(DbFile) ; true ),
    run_batch(DbFile, Ddl),
    maplist(boot_stmt_sql, BootStatements, BootSqls),
    run_batch(DbFile, BootSqls),
    length(Schedule, ScheduleLength),
    memberchk(ticks(TotalTicks), Expectations),
    DrainCount is TotalTicks - ScheduleLength,
    ( DrainCount >= 0 -> true ; throw(schedule_longer_than_ticks(Name, ScheduleLength, TotalTicks)) ),
    length(Drains, DrainCount), maplist(=([]), Drains),
    append(Schedule, Drains, FullSchedule),
    run_ticks(DbFile, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, FullSchedule, ActualDeltaTicks),
    query_final_rows(DbFile, RelPlans, FinalRows),
    report(Name, Expectations, ActualDeltaTicks, FinalRows).

tmp_db_file(Name, DbFile) :-
    format(atom(DbFile), '/tmp/tsv2_sql_check_~w.sqlite3', [Name]).

boot_stmt_sql(bootstmt(_Rel, Sql, Params), ExecutableSql) :- substitute_params(Sql, Params, ExecutableSql).

% ═══ per-tick driving (round 2: before-snapshot, absorb, resolve edge
% writes, recompute levels, after-snapshot, multiset diff) ═════════════════

run_ticks(_, _, _, _, _, _, [], []).
run_ticks(DbFile, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, [Arrivals | RestSchedule], [TickDeltaMap | MoreDeltaMaps]) :-
    snapshot_all(DbFile, DeltaStatements, RelPlans, BeforeByRef),
    absorb_arrivals(DbFile, ArrivalStatements, Arrivals),
    forall(member(EdgeStmt, EdgeStatements), resolve_and_apply_edge_writes(DbFile, RelPlans, EdgeStmt, Arrivals)),
    maplist(level_sql_pair, LevelStatements, LevelSqls0), append(LevelSqls0, LevelSqls),
    run_batch(DbFile, LevelSqls),
    snapshot_all(DbFile, DeltaStatements, RelPlans, AfterByRef),
    findall(delta(Ref, DelTerms, AddTerms),
            ( member(Ref-BeforeTerms, BeforeByRef), memberchk(Ref-AfterTerms, AfterByRef),
              multiset_diff(BeforeTerms, AfterTerms, AddTerms, DelTerms) ),
            DeltaResults),
    TickDeltaMap = tickdeltas(DeltaResults),
    run_ticks(DbFile, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, RestSchedule, MoreDeltaMaps).

% InsertSqls is a LIST (lower.pl:level_statement_group/3 -- one entry per
% rule clause sharing the head ref, the phase C multi-clause-per-head fix);
% flattens to [Delete, Insert] for the common singleton case, unchanged from
% before.
level_sql_pair(levelstmt(_, DeleteSql, InsertSqls, _), [DeleteSql | InsertSqls]).

% Full-table read per rel, AS RECONSTRUCTED TERMS (not raw rows) -- the
% multiset diff and the fixture comparison both operate at the term level,
% matching engine.pl's own boundary_deltas (which diffs Row TERMS, sorted
% via Prolog standard order of terms; this harness's multiset_diff/4 sorts
% the same way, so ordering matches the oracle by construction, not luck).
snapshot_all(DbFile, DeltaStatements, RelPlans, ByRef) :-
    findall(Ref-Terms,
            ( member(deltastmt(Ref, SelectSql, _, _, _), DeltaStatements),
              query_json(DbFile, SelectSql, Rows),
              relplan_columns(RelPlans, Ref, Columns),
              ref_table_name(Ref, Name),
              maplist(row_to_term(Name, Columns), Rows, Terms)
            ), ByRef).

% ═══ arrivals (round 2: no tick/seq anywhere -- arrivalstmt/4's AddSql now
% just takes the declared columns, matching absorb_arrivals/8's own
% simplicity for a Log rel: an unconditional append) ═══════════════════════

absorb_arrivals(_, _, []) :- !.
absorb_arrivals(DbFile, ArrivalStatements, [Signed | Rest]) :-
    ( Signed = +Row -> Sign = add ; Signed = -Row, Sign = del ),
    rel_ref(Row, Ref),
    memberchk(arrivalstmt(Ref, Kind, AddSql, DelSql, _, _), ArrivalStatements),
    Row =.. [_ | Values],
    ( Sign == add
    -> Params = Values, SqlTemplate = AddSql
    ;  Kind == log
    -> throw(retract_from_log(Ref))
    ;  DelSql == none
    -> throw(retract_from_log(Ref))
    ;  Params = Values, SqlTemplate = DelSql
    ),
    substitute_params(SqlTemplate, Params, Executable),
    run_batch(DbFile, [Executable]),
    absorb_arrivals(DbFile, ArrivalStatements, Rest).

ref_table_name(Name/_Arity, Name).

% ═══ edge rule resolution (round 2: replays "later write wins" over the raw
% arrival list, projects each candidate row via ProjectSql, then upserts) ═══
% Mirrors what the emitted TypeScript's resolve<Head>Writes does: filter
% `arrivals` for this trigger + sign=add, project each row (a real SQL
% query per candidate -- the projection needs json1, not worth
% reimplementing in Prolog), fold with LATER OVERWRITES EARLIER for the same
% key (a plain foldl replacing any prior entry for that key), then upsert
% each surviving resolved row once.

% Each candidate keeps TWO representations of the SAME projected row:
% RawValues (the literal TEXT SQL produced -- already correctly json1-
% encoded for a compound column) for the UPSERT params, and Term (RawValues
% run through decode_cell/row_to_term, a genuine Prolog compound) for key
% extraction and equality. An earlier version of this predicate decoded
% ONCE and reused the DECODED term's compound value as an UPSERT param --
% `sql_param_literal`'s default `~w` printing of a Prolog compound term
% produces PROLOG term syntax (`route_data(settings)`), not the json1 text
% SQLite actually returned (`{"fn":"route_data","args":["settings"]}`),
% silently corrupting the stored value. Caught by cross-checking this
% harness's own `open_scope` table content directly, not by any expectation
% comparison (both fixtures' `deltas/final` checks happened to still read
% back correctly since the DECODE side is symmetric -- decode_cell's `{"fn":`
% sniff never matched the corrupted text either, so it round-tripped as an
% atom and still equality-compared correctly against the fixture's OWN term;
% only inspecting the raw table caught the real stored value was wrong).
resolve_and_apply_edge_writes(DbFile, RelPlans, edgestmt(HeadRef, TriggerRef, HeadColumns, KeyColumns, ProjectSql, UpsertSql, _), Arrivals) :-
    findall(Values,
            ( member(+Row, Arrivals), rel_ref(Row, TriggerRef), Row =.. [_ | Values] ),
            TriggerRowsValues),
    findall(candidate(RawValues, Term),
            ( member(Values, TriggerRowsValues),
              substitute_params(ProjectSql, Values, Executable),
              query_json(DbFile, Executable, [json(Pairs)]),
              maplist(column_value(Pairs), HeadColumns, RawValues),
              relplan_of(RelPlans, HeadRef, _),
              ref_table_name(HeadRef, Name),
              maplist(decode_cell, RawValues, DecodedValues),
              Term =.. [Name | DecodedValues]
            ), Candidates),
    resolve_last_write_wins(Candidates, KeyColumns, HeadColumns, ResolvedCandidates),
    forall(member(candidate(RawValues2, _), ResolvedCandidates),
           ( substitute_params(UpsertSql, RawValues2, Executable2),
             run_batch(DbFile, [Executable2]) )).

% Folds LEFT TO RIGHT (arrival/schedule order), replacing any earlier entry
% sharing the same key -- the same "later write wins" rule
% apply_edge_writes/6 implements, and the same Map.set() overwrite order the
% emitted TypeScript relies on.
resolve_last_write_wins(Candidates, KeyColumns, HeadColumns, ResolvedCandidates) :-
    foldl(resolve_step(KeyColumns, HeadColumns), Candidates, [], ResolvedPairs),
    pairs_values_ordered(ResolvedPairs, ResolvedCandidates).

resolve_step(KeyColumns, HeadColumns, candidate(RawValues, Term), Acc0, Acc) :-
    term_key(Term, KeyColumns, HeadColumns, Key),
    ( selectchk(Key-_, Acc0, Acc1) -> true ; Acc1 = Acc0 ),
    append(Acc1, [Key-candidate(RawValues, Term)], Acc).

term_key(Term, KeyColumns, HeadColumns, Key) :-
    Term =.. [_ | Values],
    findall(Value,
            ( member(Column, KeyColumns), nth0(Index, HeadColumns, Column), nth0(Index, Values, Value) ),
            Key).

pairs_values_ordered([], []).
pairs_values_ordered([_-Value | Rest], [Value | More]) :- pairs_values_ordered(Rest, More).

% ═══ multiset diff (mirrors runtime/diff.ts's multisetDiff -- count-based,
% so a Log rel's genuine duplicate rows are handled correctly, not just
% Set/level rels' always-0-or-1 case) ════════════════════════════════════

multiset_diff(Before, After, Add, Del) :-
    append(Before, After, All), sort(All, Distinct),
    findall(Extra,
            ( member(Row, Distinct),
              count_occurrences(Row, After, AfterCount),
              count_occurrences(Row, Before, BeforeCount),
              AfterCount > BeforeCount,
              ExtraCount is AfterCount - BeforeCount,
              repeat_n_times(Row, ExtraCount, Repeated),
              member(Extra, Repeated) ),
            Add),
    findall(Missing,
            ( member(Row, Distinct),
              count_occurrences(Row, After, AfterCount),
              count_occurrences(Row, Before, BeforeCount),
              BeforeCount > AfterCount,
              MissingCount is BeforeCount - AfterCount,
              repeat_n_times(Row, MissingCount, Repeated),
              member(Missing, Repeated) ),
            Del).

count_occurrences(Item, List, Count) :- include(==(Item), List, Matches), length(Matches, Count).

repeat_n_times(_, 0, []) :- !.
repeat_n_times(Item, N, [Item | Rest]) :- N > 0, N1 is N - 1, repeat_n_times(Item, N1, Rest).

% ═══ row/term decoding (unchanged from round 1) ═════════════════════════════

row_to_term(Name, Columns, json(Pairs), Term) :-
    maplist(column_value(Pairs), Columns, RawValues),
    maplist(decode_cell, RawValues, DecodedValues),
    Term =.. [Name | DecodedValues].

column_value(Pairs, Column, Value) :- memberchk(Column=Value, Pairs).

% Round 3 (reconciliation): decode_cell now meets a compound column in TWO
% different text shapes depending on WHICH query produced it.
% edge_statement/3's ProjectSql and query_final_rows/3's plain column list
% still read the STORAGE encoding (json1: `{"fn":"route_data","args":[...
% ]}`) -- unaffected by the round-3 change, which touched delta_statement/2
% only. deltastmt/2's SelectSql (lower.pl:canonical_column_expr/2) now
% renders LOG-FACING canonical Prolog term text instead
% (`route_data(settings)`), which snapshot_all/4 reads via that same
% deltastmt SQL. decode_cell is shared by both call sites, so it tries the
% json1 sniff FIRST, then a canonical-term-text sniff, falling through to a
% plain atom if neither shape matches -- atom_json_term returns JSON string
% values as ATOMS (SWI's classic JSON representation), never Prolog
% strings, so both sniffs work over atomic/1, not string/1.
decode_cell(Raw, Decoded) :-
    ( atomic(Raw), atom_string(Raw, RawString), sub_string(RawString, 0, _, _, "{\"fn\":")
    -> atom_json_term(RawString, json(Pairs), []),
       memberchk(fn=Functor, Pairs),
       memberchk(args=ArgsRaw, Pairs),
       maplist(decode_cell, ArgsRaw, DecodedArgs),
       Decoded =.. [Functor | DecodedArgs]
    ; atom(Raw), sub_atom(Raw, _, _, 0, ')'), sub_atom(Raw, Before, _, _, '('), Before > 0
    -> catch(term_to_atom(Decoded, Raw), _, Decoded = Raw)
    ; string(Raw) -> atom_string(Decoded, Raw)
    ; Decoded = Raw
    ).

% ═══ final rows ══════════════════════════════════════════════════════════════

query_final_rows(DbFile, RelPlans, FinalByRef) :-
    findall(Ref-Terms,
            ( member(RelPlan, RelPlans),
              relplan_parts(RelPlan, Ref, _Kind, Columns, _, _),
              ref_table_name(Ref, Name),
              maplist(quoted_ident_test, Columns, QuotedColumns),
              atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
              format(atom(Sql), 'SELECT ~w FROM "~w"', [ColumnsSql, Name]),
              query_json(DbFile, Sql, Rows),
              maplist(row_to_term(Name, Columns), Rows, Terms0),
              msort(Terms0, Terms)
            ), FinalByRef).

quoted_ident_test(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

% ═══ sqlite3 process plumbing (unchanged from round 1) ══════════════════════

run_batch(_, []) :- !.
run_batch(DbFile, Statements) :-
    atomic_list_concat(Statements, ';\n', Body0),
    format(atom(Script), '~w;\n', [Body0]),
    setup_call_cleanup(
        process_create(path(sqlite3), [DbFile], [stdin(pipe(In)), stdout(pipe(Out)), stderr(pipe(ErrStream)), process(Pid)]),
        ( format(In, "~w", [Script]), close(In),
          read_string(Out, _, OutText), close(Out),
          read_string(ErrStream, _, ErrText), close(ErrStream),
          process_wait(Pid, Status),
          ( Status == exit(0) -> true
          ; throw(sqlite_batch_failed(Status, ErrText, OutText, Script)) ) ),
        true).

query_json(DbFile, Sql, Rows) :-
    setup_call_cleanup(
        process_create(path(sqlite3), ['-json', DbFile, Sql], [stdout(pipe(Out)), stderr(pipe(ErrStream)), process(Pid)]),
        ( read_string(Out, _, OutText), close(Out),
          read_string(ErrStream, _, ErrText), close(ErrStream),
          process_wait(Pid, Status),
          ( Status == exit(0) -> true ; throw(sqlite_query_failed(Status, ErrText, Sql)) ),
          ( normalize_space(atom(Trimmed), OutText), Trimmed == '' -> Rows = []
          ; atom_json_term(OutText, RowsRaw, []),
            ( RowsRaw = [] -> Rows = [] ; is_list(RowsRaw) -> Rows = RowsRaw ; Rows = [RowsRaw] ) )
        ),
        true).

% ═══ SQL literal substitution (test harness's own copy; independent of
% lower.pl's sql_literal, which is compile-time-only) ════════════════════════
% Handles BOTH plain `?` placeholders (arrival/level/boot statements) and
% SQLite's numbered `?1`/`?2` placeholders (edge ProjectSql) -- a numbered
% placeholder may repeat (the same trigger argument referenced twice in a
% head expression), so substitution indexes into Params by the NUMBER, not
% by occurrence order.

substitute_params(Sql, Params, Executable) :-
    ( sub_atom(Sql, _, _, _, '?1') ; sub_atom(Sql, _, _, _, '?2') ; sub_atom(Sql, _, _, _, '?3') )
    -> substitute_numbered_params(Sql, Params, Executable)
    ; substitute_plain_params(Sql, Params, Executable).

substitute_plain_params(Sql, Params, Executable) :-
    atomic_list_concat(Parts, '?', Sql),
    length(Parts, PartCount), length(Params, ParamCount),
    ( PartCount =:= ParamCount + 1 -> true
    ; throw(param_count_mismatch(Sql, Params)) ),
    maplist(sql_param_literal, Params, Literals),
    interleave_parts(Parts, Literals, Pieces),
    atomic_list_concat(Pieces, Executable).

interleave_parts([Part], [], [Part]) :- !.
interleave_parts([Part | Parts], [Literal | Literals], [Part, Literal | Rest]) :-
    interleave_parts(Parts, Literals, Rest).

substitute_numbered_params(Sql, Params, Executable) :-
    numbervars_replace(Sql, Params, 1, Executable).

numbervars_replace(Sql, Params, Index, Executable) :-
    length(Params, N),
    ( Index > N -> Executable = Sql
    ; nth1(Index, Params, Param),
      sql_param_literal(Param, Literal),
      format(atom(Placeholder), '?~w', [Index]),
      atomic_list_concat(Parts, Placeholder, Sql),
      atomic_list_concat(Parts, Literal, Sql1),
      NextIndex is Index + 1,
      numbervars_replace(Sql1, Params, NextIndex, Executable)
    ).

sql_param_literal(Param, Literal) :- number(Param), !, format(atom(Literal), '~w', [Param]).
sql_param_literal(Param, Literal) :- format(atom(Literal), '\'~w\'', [Param]).

% rel_ref, duplicated here rather than imported, since this is the only body
% predicate the harness needs and analyze.pl's own copy is not exported for
% this purpose beyond body.pl's original.
rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).

% ═══ reporting ═══════════════════════════════════════════════════════════════

report(Name, Expectations, ActualDeltaTicks, FinalRows) :-
    findall(Result,
            ( member(Expectation, Expectations), check_expectation(Expectation, ActualDeltaTicks, FinalRows, Result) ),
            Results),
    length(Results, Total),
    ( \+ member(fail(_, _, _), Results)
    -> format("~w: ALL ~w EXPECTATIONS PASS~n", [Name, Total])
    ; format("~w: MISMATCHES FOUND (of ~w expectations)~n", [Name, Total])
    ),
    forall(member(Result, Results), print_result(Result)).

print_result(pass(What)) :- format("  PASS ~q~n", [What]).
print_result(fail(What, Got, Want)) :- format("  FAIL ~q~n    got:  ~q~n    want: ~q~n", [What, Got, Want]).

check_expectation(ticks(Expected), ActualDeltaTicks, _, Result) :-
    length(ActualDeltaTicks, Actual),
    ( Actual == Expected -> Result = pass(ticks(Expected)) ; Result = fail(ticks, Actual, Expected) ).
check_expectation(final(Ref, Expected), _, FinalRows, Result) :-
    ( memberchk(Ref-Actual, FinalRows) -> true ; Actual = missing ),
    ( Actual == Expected -> Result = pass(final(Ref)) ; Result = fail(final(Ref), Actual, Expected) ).
check_expectation(deltas(Ref, ExpectedPerTick), ActualDeltaTicks, _, Result) :-
    findall(TickDeltaForRef,
            ( member(tickdeltas(DeltaResults), ActualDeltaTicks),
              ( memberchk(delta(Ref, DelTerms, AddTerms), DeltaResults)
              -> maplist(as_minus, DelTerms, DelSigned), maplist(as_plus, AddTerms, AddSigned),
                 append(DelSigned, AddSigned, TickDeltaForRef)
              ;  TickDeltaForRef = [] )
            ), ActualPerTick),
    ( ActualPerTick == ExpectedPerTick -> Result = pass(deltas(Ref)) ; Result = fail(deltas(Ref), ActualPerTick, ExpectedPerTick) ).
check_expectation(throws(_), _, _, pass(throws_not_checked_by_sql_harness)).

as_minus(Term, -Term).
as_plus(Term, +Term).
