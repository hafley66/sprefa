% diag_emit.pl : close the self-programmable-LSP loop from the prolog side.
% Prolog-side checks over the v6 prolog tree write rows into a sqlite file whose
% `diag_v5` view is EXACTLY the interface v5's `dl --lsp --diag-db` already
% polls (src/lsp.rs:545). No new editor code, no new rust code: the editor
% squiggles come from a view this file creates.
%
% Run:  swipl -q -l v6/prolog/labs/diag_emit.pl -g go -g halt
%
% swipl has no sqlite driver, so every write is SQL TEXT piped to the sqlite3
% CLI through one process_create. The law that matters: ONE process per emit.
% A per-row spawn is the N+1 defect in process clothing, and check
% `one_sqlite_process_per_emit` grades it by counting the wrapper's calls.
%
% Refresh semantics are level semantics done by hand: an emit is told which
% SOURCE PATHS it scanned, DELETEs every row for those paths, then inserts what
% the checks found this run. A finding that got fixed disappears; rows for paths
% this run did not scan are untouched. That is what makes the squiggles retract.
%
% The three checks are real (they read the tree with read_file_to_string and
% report honest line numbers), but the grader runs them over FIXTURE files
% written under labs/out/fixtures so PASS does not depend on the current state
% of the repo. The live tree gets one smoke emit whose row count is unasserted.
%
% Positions: the view's contract is 0-BASED (src/lsp.rs:395-398, and
% diag_v5_to_diagnostic at :598 does no adjustment), so the first line of a file
% is line 0 here. See diag_emit.md ambiguity 1.

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module(library(filesex)).
:- use_module('../src/grader.pl').

% The lab's own directory, captured at load time so the lab runs from any cwd.
:- prolog_load_context(directory, LabDirectory),
   nb_setval(lab_directory, LabDirectory).

% ═══ 1. the row shape, taken from src/lsp.rs ═════════════════════════════════
%
% src/lsp.rs:545 issues, verbatim:
%   SELECT path, line, col, end_line, end_col, severity, code, msg, hint
%   FROM diag_v5
% and src/lsp.rs:399-409 reads them positionally into DiagV5Row as
%   String, i64, i64, i64, i64, String, String, String, Option<String>.
% So: 9 columns, this order, hint the only nullable one.

diag_v5_column(0, path,     text).
diag_v5_column(1, line,     integer).
diag_v5_column(2, col,      integer).
diag_v5_column(3, end_line, integer).
diag_v5_column(4, end_col,  integer).
diag_v5_column(5, severity, text).
diag_v5_column(6, code,     text).
diag_v5_column(7, msg,      text).
diag_v5_column(8, hint,     text).

diag_v5_column_names(Names) :-
    findall(Name, (between(0, 8, Index), diag_v5_column(Index, Name, _)), Names).

% The term shape a check produces. Arity and argument order are the view's
% column order, so row_tuple/2 is a positional walk with no name lookup.
%   diag(Path, Line, Col, EndLine, EndCol, Severity, Code, Msg, Hint)
% Hint is the atom `null` when absent (the only column lsp.rs types as Option).

diag_term_functor(diag, 9).

% src/lsp.rs:585-593: error/warn/info/hint map to ERROR/WARNING/INFORMATION/HINT
% and every other string silently becomes WARNING. A row emitted with any other
% severity is a typo the editor will not report, so this table is the vocabulary.
lsp_severity(error).
lsp_severity(warn).
lsp_severity(info).
lsp_severity(hint).

% ═══ 2. the dish: SQL text piped to the sqlite3 CLI ══════════════════════════

sqlite3_binary(Binary) :-
    member(Candidate, ['/opt/homebrew/bin/sqlite3', '/usr/bin/sqlite3', '/usr/local/bin/sqlite3']),
    exists_file(Candidate),
    !,
    Binary = Candidate.

% Process-call counter. Every sqlite3 spawn in this lab goes through
% sqlite_run/3, so this number IS the process count.
:- nb_setval(sqlite_call_count, 0).

reset_sqlite_call_count :- nb_setval(sqlite_call_count, 0).

sqlite_call_count(Count) :- nb_getval(sqlite_call_count, Count).

bump_sqlite_call_count :-
    nb_getval(sqlite_call_count, Previous),
    Next is Previous + 1,
    nb_setval(sqlite_call_count, Next).

% One sqlite3 process: write the whole script to stdin, close it, drain stdout.
% Safe against deadlock only because sqlite3 emits nothing while consuming a
% DDL/INSERT script and the query scripts here are one statement (diag_emit.md,
% limitation 3).
sqlite_run(DbPath, SqlText, OutputLines) :-
    sqlite3_binary(Binary),
    bump_sqlite_call_count,
    process_create(Binary, [DbPath],
                   [ stdin(pipe(Input)), stdout(pipe(Output)),
                     stderr(pipe(Errors)), process(Pid) ]),
    set_stream(Input, encoding(utf8)),
    set_stream(Output, encoding(utf8)),
    write(Input, SqlText),
    close(Input),
    read_stream_lines(Output, OutputLines),
    close(Output),
    read_stream_lines(Errors, ErrorLines),
    close(Errors),
    process_wait(Pid, Status),
    (   Status == exit(0)
    ->  true
    ;   throw(error(sqlite3_failed(Status, ErrorLines), _))
    ).

read_stream_lines(Stream, Lines) :-
    read_line_to_string(Stream, Line),
    (   Line == end_of_file
    ->  Lines = []
    ;   Lines = [Line | Rest],
        read_stream_lines(Stream, Rest)
    ).

% sqlite3's default non-interactive output is list mode, `|` separated, no
% header row.
sqlite_query(DbPath, SqlText, Rows) :-
    sqlite_run(DbPath, SqlText, Lines),
    maplist(split_pipe_row, Lines, Rows).

split_pipe_row(Line, Fields) :- split_string(Line, "|", "", Fields).

% SQL literal: NULL for the atom `null`, otherwise a quoted string with every
% apostrophe doubled.
sql_literal(Value, "NULL") :- Value == null, !.
sql_literal(Value, Literal) :-
    text_to_string(Value, AsString),
    split_string(AsString, "'", "", Pieces),
    atomic_list_concat(Pieces, "''", Escaped),
    format(string(Literal), "'~w'", [Escaped]).

% ═══ 3. the emitter ══════════════════════════════════════════════════════════
%
% Table name `rel_diag` and the view body are copied from the node-side v6
% runtime (v6/dl/src/5_diag.ts:39-41) so both writers agree on one shape.

diag_ddl_sql(Sql) :-
    Sql = "CREATE TABLE IF NOT EXISTS rel_diag (\c
path TEXT, line INTEGER, col INTEGER, end_line INTEGER, end_col INTEGER, \c
severity TEXT, code TEXT, msg TEXT, hint TEXT);\n\c
CREATE VIEW IF NOT EXISTS diag_v5 AS SELECT path, line, col, end_line, end_col, \c
COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag;\n".

delete_sql([], "") :- !.
delete_sql(ScannedPaths, Sql) :-
    maplist(sql_literal, ScannedPaths, Literals),
    atomic_list_concat(Literals, ",", Joined),
    format(string(Sql), "DELETE FROM rel_diag WHERE path IN (~w);\n", [Joined]).

insert_sql([], "") :- !.
insert_sql(DiagRows, Sql) :-
    maplist(row_tuple, DiagRows, Tuples),
    atomic_list_concat(Tuples, ",\n", Joined),
    format(string(Sql),
           "INSERT INTO rel_diag \c
(path, line, col, end_line, end_col, severity, code, msg, hint) VALUES\n~w;\n",
           [Joined]).

row_tuple(diag(Path, Line, Col, EndLine, EndCol, Severity, Code, Msg, Hint), Tuple) :-
    maplist(sql_literal, [Path, Severity, Code, Msg, Hint],
            [QuotedPath, QuotedSeverity, QuotedCode, QuotedMsg, QuotedHint]),
    format(string(Tuple), "(~w,~w,~w,~w,~w,~w,~w,~w,~w)",
           [QuotedPath, Line, Col, EndLine, EndCol,
            QuotedSeverity, QuotedCode, QuotedMsg, QuotedHint]).

% emit_diags(+DbPath, +ScannedPaths, +DiagRows)
% Full refresh per source: DDL, then one transaction that clears every scanned
% path and reinserts this run's findings. One sqlite3 process, always.
emit_diags(DbPath, ScannedPaths, DiagRows) :-
    diag_ddl_sql(Ddl),
    delete_sql(ScannedPaths, DeleteSql),
    insert_sql(DiagRows, InsertSql),
    atomics_to_string([Ddl, "BEGIN;\n", DeleteSql, InsertSql, "COMMIT;\n"], Sql),
    sqlite_run(DbPath, Sql, _).

% Run every check over every path, then emit once.
emit_over_files(DbPath, Paths) :-
    maplist(file_diags, Paths, DiagLists),
    append(DiagLists, DiagRows),
    emit_diags(DbPath, Paths, DiagRows).

diag_rows_for_path(DbPath, Path, Rows) :-
    sql_literal(Path, Literal),
    format(string(Sql),
           "SELECT path, line, col, end_line, end_col, severity, code, msg, hint \c
FROM diag_v5 WHERE path = ~w ORDER BY line, col;\n", [Literal]),
    sqlite_query(DbPath, Sql, Rows).

% ═══ 4. the three checks ═════════════════════════════════════════════════════
%
% (a) banned words, (b) a lab .pl with no `go :-` entry point, (c) an em dash in
% a .md. All three read real file text and report real line numbers.

% The banned-word table is spelled in halves on purpose: written whole, this
% file would trip its own check on every live run and would itself break the
% LANG.md law it enforces. diag_emit.md, limitation 1.
banned_halves(prov,    enance,  'source or origin').
banned_halves(subst,   rate,    'base layer').
banned_halves('load-', bearing, critical).
banned_halves('load_', bearing, critical).
banned_halves(reg,     ime,     mode).

banned_word(Word, Replacement) :-
    banned_halves(Left, Right, Replacement),
    atom_concat(Left, Right, Word).

% A line carrying the waiver marker is skipped. The marker text is itself
% assembled, for the same reason the word table is.
banned_waiver_marker(Marker) :- atom_concat('@banned', '-ok', Marker).

banned_word_diags(Path, Text, Diags) :-
    text_lines(Text, Lines),
    banned_waiver_marker(Marker),
    atom_string(Marker, MarkerString),
    findall(Diag,
            ( nth0(LineIndex, Lines, LineText),
              \+ sub_string(LineText, _, _, _, MarkerString),
              string_lower(LineText, LowerLine),
              banned_word(Word, Replacement),
              atom_string(Word, WordString),
              string_length(WordString, WordLength),
              sub_string(LowerLine, Col, WordLength, _, WordString),
              banned_word_diag(Path, LineIndex, Col, WordLength, Word, Replacement, Diag)
            ),
            Diags).

banned_word_diag(Path, LineIndex, Col, WordLength, Word, Replacement,
                 diag(Path, LineIndex, Col, LineIndex, EndCol, error,
                      'banned-word', Msg, Hint)) :-
    EndCol is Col + WordLength,
    format(string(Msg), "banned word '~w' in prose or identifier", [Word]),
    format(string(Hint), "write '~w' instead (LANG.md lab style law)", [Replacement]).

% (b) a lab .pl with no entry point. Anchored at the top of the file because the
% finding is about the file, not a line.
missing_go_diags(Path, Text, Diags) :-
    (   has_go_entrypoint(Text)
    ->  Diags = []
    ;   Diags = [ diag(Path, 0, 0, 0, 0, warn, 'no-go-entrypoint',
                       "lab defines no 'go :-' clause; \c
'swipl -q -l <lab>.pl -g go -g halt' cannot score it",
                       "add 'go :- run(check).' and a check/2 table") ]
    ).

has_go_entrypoint(Text) :- sub_string(Text, _, _, _, "go :-"), !.
has_go_entrypoint(Text) :- sub_string(Text, _, _, _, "go:-").

% (c) em dash in a .md. Written as a code point so this source file never has to
% contain the character it is hunting.
em_dash_diags(Path, Text, Diags) :-
    char_code(EmDash, 0x2014),
    atom_string(EmDash, EmDashString),
    text_lines(Text, Lines),
    findall(diag(Path, LineIndex, Col, LineIndex, EndCol, warn, 'em-dash',
                 "em dash in a .md file", null),
            ( nth0(LineIndex, Lines, LineText),
              sub_string(LineText, Col, 1, _, EmDashString),
              EndCol is Col + 1
            ),
            Diags).

text_lines(Text, Lines) :- split_string(Text, "\n", "\r", Lines).

% file_diags(+Path, -DiagRows): dispatch by extension.
file_diags(Path, DiagRows) :-
    read_file_to_string(Path, Text, [encoding(utf8)]),
    file_name_extension(_, Extension, Path),
    (   Extension == pl
    ->  banned_word_diags(Path, Text, BannedDiags),
        missing_go_diags(Path, Text, GoDiags),
        append(BannedDiags, GoDiags, DiagRows)
    ;   Extension == md
    ->  banned_word_diags(Path, Text, BannedDiags),
        em_dash_diags(Path, Text, EmDashDiags),
        append(BannedDiags, EmDashDiags, DiagRows)
    ;   DiagRows = []
    ).

% ═══ 5. walking the real tree ════════════════════════════════════════════════

% `out` is this lab's generated output (fixtures + db); scanning it would make
% the live run report on files the lab just wrote.
skipped_directory(out).
skipped_directory('.git').
skipped_directory(node_modules).

tree_file(Directory, Path) :-
    directory_files(Directory, Entries),
    member(Entry, Entries),
    \+ memberchk(Entry, ['.', '..']),
    \+ skipped_directory(Entry),
    directory_file_path(Directory, Entry, Candidate),
    (   exists_directory(Candidate)
    ->  tree_file(Candidate, Path)
    ;   Path = Candidate
    ).

checkable_files(Root, Paths) :-
    findall(Path,
            ( tree_file(Root, Path),
              file_name_extension(_, Extension, Path),
              memberchk(Extension, [pl, md])
            ),
            Unsorted),
    sort(Unsorted, Paths).

% ═══ 6. lab paths and fixtures ═══════════════════════════════════════════════

lab_directory(Directory) :- nb_getval(lab_directory, Directory).

prolog_root(Root) :-
    lab_directory(LabDirectory),
    file_directory_name(LabDirectory, Root).

out_directory(OutDirectory) :-
    lab_directory(LabDirectory),
    directory_file_path(LabDirectory, out, OutDirectory),
    make_directory_path(OutDirectory).

fixtures_directory(FixturesDirectory) :-
    out_directory(OutDirectory),
    directory_file_path(OutDirectory, fixtures, FixturesDirectory),
    make_directory_path(FixturesDirectory).

fixture_path(Name, Path) :-
    fixtures_directory(FixturesDirectory),
    directory_file_path(FixturesDirectory, Name, Path).

out_db(Name, Path) :-
    out_directory(OutDirectory),
    directory_file_path(OutDirectory, Name, Path).

write_text_file(Path, Text) :-
    setup_call_cleanup(open(Path, write, Stream, [encoding(utf8)]),
                       write(Stream, Text),
                       close(Stream)).

fresh_db(Path) :-
    (   exists_file(Path) -> delete_file(Path) ; true ).

% --- fixture texts, built so the banned words never appear in this source -----

% two violations, lines 1 and 3 (0-based)
dirty_notes_text(Text) :-
    nth_banned_word(0, FirstWord),
    nth_banned_word(4, SecondWord),
    format(string(Text),
           "# fixture notes\n\c
this paragraph mentions ~w in prose.\n\c
nothing wrong on this line.\n\c
and here is ~w again.\n", [FirstWord, SecondWord]).

clean_notes_text("# fixture notes\n\c
this paragraph mentions nothing wrong.\n\c
nothing wrong on this line.\n\c
and nothing wrong here either.\n").

% one violation on line 1, in a file the refresh of notes.md must not touch
other_notes_text(Text) :-
    nth_banned_word(1, Word),
    format(string(Text), "# other fixture\n\c
this one mentions ~w once.\n", [Word]).

nth_banned_word(Index, Word) :-
    findall(Candidate, banned_word(Candidate, _), Words),
    nth0(Index, Words, Word).

no_go_lab_text("% fixture lab with no entry point\n\c
helper(one).\n\c
helper(two).\n").

with_go_lab_text("% fixture lab with an entry point\n\c
helper(one).\n\c
check(helper_one, helper(one)).\n\c
go :- forall(check(Name, Goal), (Goal -> format(\"PASS  ~w~n\", [Name]) ; true)).\n").

% em dash on line 1 at column 4 (0-based), after the 4-char prefix "abc ".
em_dash_text(Text) :-
    char_code(EmDash, 0x2014),
    format(string(Text), "# dash fixture\nabc ~w def\n", [EmDash]).

% Write the whole fixture set and hand back the paths.
write_fixtures(notes(NotesPath), other(OtherPath), nogo(NoGoPath),
               withgo(WithGoPath), dash(DashPath)) :-
    fixture_path('notes.md', NotesPath),
    fixture_path('other.md', OtherPath),
    fixture_path('nogo.pl', NoGoPath),
    fixture_path('withgo.pl', WithGoPath),
    fixture_path('dash.md', DashPath),
    dirty_notes_text(DirtyText),   write_text_file(NotesPath, DirtyText),
    other_notes_text(OtherText),   write_text_file(OtherPath, OtherText),
    no_go_lab_text(NoGoText),      write_text_file(NoGoPath, NoGoText),
    with_go_lab_text(WithGoText),  write_text_file(WithGoPath, WithGoText),
    em_dash_text(DashText),        write_text_file(DashPath, DashText).

% The shared scenario every db-level check re-runs from scratch: fresh db,
% dirty fixtures, one emit over notes.md + other.md.
fixture_scenario(DbPath, NotesPath, OtherPath) :-
    write_fixtures(notes(NotesPath), other(OtherPath), _, _, _),
    out_db('fixture.sqlite', DbPath),
    fresh_db(DbPath),
    emit_over_files(DbPath, [NotesPath, OtherPath]).

% ═══ 7. the grader ═══════════════════════════════════════════════════════════

check(sqlite3_binary_found,
      ( sqlite3_binary(Binary), exists_file(Binary) )).

% The term shape is the view's column list, in order (src/lsp.rs:545).
check(row_term_matches_lsp_columns,
      ( diag_v5_column_names(Names),
        Names == [path, line, col, end_line, end_col, severity, code, msg, hint],
        diag_term_functor(Functor, Arity),
        length(Names, Arity),
        Example = diag(a, 0, 0, 0, 0, warn, c, m, null),
        functor(Example, Functor, Arity) )).

% Read the schema back out of the db the emitter created: base table columns,
% view columns, and the view's registration in sqlite_master.
check(table_and_view_columns_match_lsp,
      ( out_db('schema.sqlite', DbPath), fresh_db(DbPath),
        emit_diags(DbPath, [], []),
        diag_v5_column_names(Names),
        sqlite_query(DbPath, "PRAGMA table_info(rel_diag);\n", TableRows),
        column_names_of(TableRows, TableNames), TableNames == Names,
        sqlite_query(DbPath, "PRAGMA table_info(diag_v5);\n", ViewRows),
        column_names_of(ViewRows, ViewNames), ViewNames == Names,
        sqlite_query(DbPath,
                     "SELECT type FROM sqlite_master WHERE name = 'diag_v5';\n",
                     MasterRows),
        MasterRows == [["view"]] )).

% The exact statement src/lsp.rs:545 prepares must run against our db.
check(lsp_select_statement_runs,
      ( fixture_scenario(DbPath, _, _),
        sqlite_query(DbPath,
                     "SELECT path, line, col, end_line, end_col, severity, code, msg, hint \c
FROM diag_v5;\n", Rows),
        length(Rows, RowCount), RowCount >= 3,
        forall(member(Row, Rows), length(Row, 9)) )).

% Term level: the 2-violation fixture yields exactly 2 rows, right lines, right
% severity, right path.
check(fixture_two_violations,
      ( write_fixtures(notes(NotesPath), _, _, _, _),
        file_diags(NotesPath, Diags),
        length(Diags, 2),
        forall(member(diag(Path, _, _, _, _, Severity, Code, _, _), Diags),
               ( Path == NotesPath, Severity == error, Code == 'banned-word' )),
        findall(Line, member(diag(_, Line, _, _, _, _, _, _, _), Diags), Lines),
        msort(Lines, SortedLines), SortedLines == [1, 3] )).

% Db level: the same 2 rows come back through the view, lines intact.
check(fixture_rows_readable_through_view,
      ( fixture_scenario(DbPath, NotesPath, _),
        diag_rows_for_path(DbPath, NotesPath, Rows),
        length(Rows, 2),
        maplist(nth0(1), Rows, LineFields), LineFields == ["1", "3"],
        maplist(nth0(5), Rows, SeverityFields), SeverityFields == ["error", "error"] )).

% Fix the file, re-emit for that path only: its rows are gone. Level semantics.
check(refresh_clears_fixed_path,
      ( fixture_scenario(DbPath, NotesPath, _),
        clean_notes_text(CleanText), write_text_file(NotesPath, CleanText),
        emit_over_files(DbPath, [NotesPath]),
        diag_rows_for_path(DbPath, NotesPath, Rows),
        Rows == [] )).

% ... and the path the second emit did not scan keeps its row.
check(refresh_leaves_other_paths,
      ( fixture_scenario(DbPath, NotesPath, OtherPath),
        clean_notes_text(CleanText), write_text_file(NotesPath, CleanText),
        emit_over_files(DbPath, [NotesPath]),
        diag_rows_for_path(DbPath, OtherPath, Rows),
        length(Rows, 1) )).

% The N+1 rail: an emit over several files with several findings is ONE process.
check(one_sqlite_process_per_emit,
      ( write_fixtures(notes(NotesPath), other(OtherPath), nogo(NoGoPath), _, dash(DashPath)),
        out_db('batch.sqlite', DbPath), fresh_db(DbPath),
        Paths = [NotesPath, OtherPath, NoGoPath, DashPath],
        maplist(file_diags, Paths, DiagLists), append(DiagLists, DiagRows),
        length(DiagRows, Found), Found >= 5,
        reset_sqlite_call_count,
        emit_diags(DbPath, Paths, DiagRows),
        sqlite_call_count(1) )).

check(missing_go_entrypoint_fixture,
      ( write_fixtures(_, _, nogo(NoGoPath), withgo(WithGoPath), _),
        file_diags(NoGoPath, NoGoDiags),
        NoGoDiags = [diag(NoGoPath, 0, 0, 0, 0, warn, 'no-go-entrypoint', _, _)],
        file_diags(WithGoPath, WithGoDiags),
        WithGoDiags == [] )).

check(em_dash_fixture,
      ( write_fixtures(_, _, _, _, dash(DashPath)),
        file_diags(DashPath, Diags),
        Diags = [diag(DashPath, 1, 4, 1, 5, warn, 'em-dash', _, null)] )).

% Every severity we can emit is one src/lsp.rs:585-593 actually maps; anything
% else would silently render as a warning.
check(severity_vocabulary_matches_lsp_mapping,
      ( write_fixtures(notes(NotesPath), _, nogo(NoGoPath), _, dash(DashPath)),
        maplist(file_diags, [NotesPath, NoGoPath, DashPath], DiagLists),
        append(DiagLists, DiagRows),
        DiagRows \== [],
        forall(member(diag(_, _, _, _, _, Severity, _, _, _), DiagRows),
               lsp_severity(Severity)) )).

% The messages carry apostrophes, so quote doubling is exercised on every emit;
% this grades that the text survives the round trip through sqlite3 stdin.
check(apostrophe_round_trip,
      ( fixture_scenario(DbPath, NotesPath, _),
        diag_rows_for_path(DbPath, NotesPath, [FirstRow | _]),
        nth0(7, FirstRow, Msg),
        sub_string(Msg, _, _, _, "'"),
        nth_banned_word(0, FirstWord), atom_string(FirstWord, WordString),
        sub_string(Msg, _, _, _, WordString) )).

% Live smoke over the real tree: exit 0 and a queryable view. Row count is
% deliberately unasserted (it tracks whatever the repo currently says).
check(live_tree_smoke,
      ( prolog_root(Root), checkable_files(Root, Paths),
        length(Paths, FileCount), FileCount >= 5,
        out_db('diag.sqlite', DbPath), fresh_db(DbPath),
        emit_over_files(DbPath, Paths),
        sqlite_query(DbPath,
                     "SELECT count(*) FROM diag_v5;\n", [[CountField]]),
        number_string(RowCount, CountField), RowCount >= 0 )).

column_names_of(PragmaRows, Names) :-
    findall(Name,
            ( member(Row, PragmaRows), nth0(1, Row, Field), atom_string(Name, Field) ),
            Names).

go :- run(check).
