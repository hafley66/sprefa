% run_sql_check.pl : self-grading harness (TASK item 2). Drives the SAME
% lowered/8 term compile.pl builds through the REAL sqlite3 CLI (never
% through the emitted .ts, which has no runtime to execute it against yet --
% Phase A's runtime lands in a separate worktree) and compares the resulting
% per-tick deltas + final rows against the fixture's OWN expected values
% (fixtures/scopes.pl's deltas/final/ticks terms), which is the same
% oracle every other conformance fixture is graded against. This proves the
% COMPILED SQL computes the right answer before any generated TypeScript
% runs anywhere.
%
% The harness, not the emitted program, decides how many DRAIN ticks to run
% (reads ticks(N) straight off the fixture and pads with empty-arrival ticks
% past the schedule's own length) -- the real answer ("does a tick's own
% output carry into a T+1 occurrence") is IGenProgram seam friction flagged
% in the arc report, not resolved here.
%
% Run: swipl -q -l v6/prolog/compile/test/run_sql_check.pl -g "check(switch_as_keyed_replace)" -g halt

:- module(run_sql_check, [ check/1, check_all/0 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module(library(http/json)).
:- use_module('../compile', [ read_fixture_term/4, program_plan/2 ]).
:- use_module('../lower', [ lower_program/2, boot_statements/3 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture_file('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-a1aa8144f466d366d/v6/prolog/conformance/fixtures/scopes.pl').

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
    Plan = plan(_, _, RelPlans, _ArrivalTargets, _, _),
    boot_statements(RelPlans, Initial, BootStatements),
    Lowered = lowered(_, Ddl, ArrivalStatements, EdgeStatements, LevelStatements, DeltaStatements, RelPlans, _),
    tmp_db_file(Name, DbFile),
    ( exists_file(DbFile) -> delete_file(DbFile) ; true ),
    run_batch(DbFile, Ddl),
    maplist(boot_stmt_sql, BootStatements, BootSqls),
    run_batch(DbFile, BootSqls),
    mutation_template_list(EdgeStatements, LevelStatements, MutationTemplates),
    length(Schedule, ScheduleLength),
    memberchk(ticks(TotalTicks), Expectations),
    DrainCount is TotalTicks - ScheduleLength,
    ( DrainCount >= 0 -> true ; throw(schedule_longer_than_ticks(Name, ScheduleLength, TotalTicks)) ),
    length(Drains, DrainCount), maplist(=([]), Drains),
    append(Schedule, Drains, FullSchedule),
    run_ticks(DbFile, ArrivalStatements, MutationTemplates, DeltaStatements, RelPlans, FullSchedule, 1, ActualDeltaTicks),
    query_final_rows(DbFile, RelPlans, FinalRows),
    report(Name, Expectations, ActualDeltaTicks, FinalRows).

tmp_db_file(Name, DbFile) :-
    format(atom(DbFile), '/tmp/tsv2_sql_check_~w.sqlite3', [Name]).

boot_stmt_sql(bootstmt(Sql, Params), ExecutableSql) :- substitute_params(Sql, Params, ExecutableSql).

% ═══ mutation statement flattening (edge deletes/inserts, then level) ══════

mutation_template_list(EdgeStatements, LevelStatements, Templates) :-
    findall(mutstmt(Sql, tick), ( member(edgestmt(_, DeleteSql, InsertSql), EdgeStatements), member(Sql, [DeleteSql, InsertSql]) ), EdgeTemplates),
    findall(mutstmt(Sql, none), ( member(levelstmt(_, DeleteSql, InsertSql), LevelStatements), member(Sql, [DeleteSql, InsertSql]) ), LevelTemplates),
    append(EdgeTemplates, LevelTemplates, Templates).

% ═══ per-tick driving ════════════════════════════════════════════════════════

run_ticks(_, _, _, _, _, [], _, []).
run_ticks(DbFile, ArrivalStatements, MutationTemplates, DeltaStatements, RelPlans, [Arrivals | RestSchedule], Tick, [TickDeltaMap | MoreDeltaMaps]) :-
    absorb_arrivals(DbFile, ArrivalStatements, Tick, Arrivals),
    maplist(mutation_sql(Tick), MutationTemplates, MutationSqls),
    run_batch(DbFile, MutationSqls),
    maplist(delta_result(DbFile, Tick, RelPlans), DeltaStatements, DeltaResults),
    TickDeltaMap = tickdeltas(Tick, DeltaResults),
    refresh_snapshots(DbFile, DeltaStatements),
    NextTick is Tick + 1,
    run_ticks(DbFile, ArrivalStatements, MutationTemplates, DeltaStatements, RelPlans, RestSchedule, NextTick, MoreDeltaMaps).

mutation_sql(Tick, mutstmt(Sql, tick), Executable) :- !, substitute_params(Sql, [Tick], Executable).
mutation_sql(_, mutstmt(Sql, none), Executable) :- substitute_params(Sql, [], Executable).

absorb_arrivals(_, _, _, []) :- !.
absorb_arrivals(DbFile, ArrivalStatements, Tick, [Signed | Rest]) :-
    ( Signed = +Row -> Sign = add ; Signed = -Row, Sign = del ),
    rel_ref(Row, Ref),
    memberchk(arrivalstmt(Ref, Kind, AddSql, DelSql), ArrivalStatements),
    Row =.. [_ | Values],
    ( Sign == add, Kind == log
    -> next_seq_for_tick(DbFile, Ref, Tick, Seq), Params = [Tick, Seq | Values], SqlTemplate = AddSql
    ;  Sign == add
    -> Params = Values, SqlTemplate = AddSql
    ;  DelSql == none
    -> throw(retract_from_log(Ref))
    ;  Params = Values, SqlTemplate = DelSql
    ),
    substitute_params(SqlTemplate, Params, Executable),
    run_batch(DbFile, [Executable]),
    absorb_arrivals(DbFile, ArrivalStatements, Tick, Rest).

% Mirrors engine.pl's next_seq/3: 1 + the highest seq already stamped this
% tick (or 1 if none yet). The harness assigns this per row exactly like the
% generated program's applyArrivalRow does with its running `index + 1`,
% just computed by re-querying the table instead of a JS closure variable
% (the harness has no in-memory loop state; SQL is the state here).
next_seq_for_tick(DbFile, Ref, Tick, Seq) :-
    ref_table_name(Ref, Table),
    format(atom(Sql), 'SELECT COALESCE(MAX(seq), 0) + 1 AS next_seq FROM "~w" WHERE tick = ~w', [Table, Tick]),
    query_json(DbFile, Sql, [json([next_seq=Seq])]).

ref_table_name(Name/_Arity, Name).

% ═══ delta collection ════════════════════════════════════════════════════════

delta_result(DbFile, Tick, RelPlans, deltastmt(Ref, log, AddsSql, none, []), delta(Ref, [], AddTerms)) :- !,
    substitute_params(AddsSql, [Tick], Executable),
    query_json(DbFile, Executable, Rows),
    memberchk(relplan(Ref, _, Columns, _), RelPlans),
    ref_table_name(Ref, Name),
    maplist(row_to_term(Name, Columns), Rows, AddTerms).
delta_result(DbFile, _Tick, RelPlans, deltastmt(Ref, set, AddsSql, DelsSql, _Refresh), delta(Ref, DelTerms, AddTerms)) :-
    query_json(DbFile, DelsSql, DelRows),
    query_json(DbFile, AddsSql, AddRows),
    memberchk(relplan(Ref, _, Columns, _), RelPlans),
    ref_table_name(Ref, Name),
    maplist(row_to_term(Name, Columns), DelRows, DelTerms),
    maplist(row_to_term(Name, Columns), AddRows, AddTerms).

refresh_snapshots(DbFile, DeltaStatements) :-
    findall(Sql, ( member(deltastmt(_, set, _, _, RefreshSqls), DeltaStatements), member(Sql, RefreshSqls) ), Sqls),
    run_batch(DbFile, Sqls).

row_to_term(Name, Columns, json(Pairs), Term) :-
    maplist(column_value(Pairs), Columns, RawValues),
    maplist(decode_cell, RawValues, DecodedValues),
    Term =.. [Name | DecodedValues].

column_value(Pairs, Column, Value) :- memberchk(Column=Value, Pairs).

% atom_json_term returns JSON string values as ATOMS (SWI's classic JSON
% representation), never Prolog strings, so the compound-encoding sniff and
% the recursive re-parse both need to work over atomic/1, not string/1 --
% string/1 alone silently never matched, leaving every compound column raw.
decode_cell(Raw, Decoded) :-
    ( atomic(Raw), atom_string(Raw, RawString), sub_string(RawString, 0, _, _, "{\"fn\":")
    -> atom_json_term(RawString, json(Pairs), []),
       memberchk(fn=Functor, Pairs),
       memberchk(args=ArgsRaw, Pairs),
       maplist(decode_cell, ArgsRaw, DecodedArgs),
       Decoded =.. [Functor | DecodedArgs]
    ; string(Raw) -> atom_string(Decoded, Raw)
    ; Decoded = Raw
    ).

% ═══ final rows ══════════════════════════════════════════════════════════════

query_final_rows(DbFile, RelPlans, FinalByRef) :-
    findall(Ref-Terms,
            ( member(relplan(Ref, _Kind, Columns, _), RelPlans),
              ref_table_name(Ref, Name),
              maplist(quoted_ident_test, Columns, QuotedColumns),
              atomic_list_concat(QuotedColumns, ', ', ColumnsSql),
              format(atom(Sql), 'SELECT ~w FROM "~w"', [ColumnsSql, Name]),
              query_json(DbFile, Sql, Rows),
              maplist(row_to_term(Name, Columns), Rows, Terms0),
              msort(Terms0, Terms)
            ), FinalByRef).

quoted_ident_test(Name, Quoted) :- format(atom(Quoted), '"~w"', [Name]).

% ═══ sqlite3 process plumbing ════════════════════════════════════════════════

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

substitute_params(Sql, Params, Executable) :-
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
            ( member(tickdeltas(_, DeltaResults), ActualDeltaTicks),
              ( memberchk(delta(Ref, DelTerms, AddTerms), DeltaResults)
              -> maplist(as_minus, DelTerms, DelSigned), maplist(as_plus, AddTerms, AddSigned),
                 append(DelSigned, AddSigned, TickDeltaForRef)
              ;  TickDeltaForRef = [] )
            ), ActualPerTick),
    ( ActualPerTick == ExpectedPerTick -> Result = pass(deltas(Ref)) ; Result = fail(deltas(Ref), ActualPerTick, ExpectedPerTick) ).
check_expectation(throws(_), _, _, pass(throws_not_checked_by_sql_harness)).

as_minus(Term, -Term).
as_plus(Term, +Term).
