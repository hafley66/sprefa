% ╔══════════════════════════════════════════════════════════════════════════╗
% ║  PROTOTYPE LOWERING -- LAB ONLY, NOT PRODUCTION.                         ║
% ║  Not consulted by compile/lower.pl. It emits SQL text and runs it        ║
% ║  against the REAL system sqlite3 so the lowering claims are measured,    ║
% ║  not asserted.                                                          ║
% ╚══════════════════════════════════════════════════════════════════════════╝
%
% THE COEXISTENCE RULE this file demonstrates (§3 of the plan doc):
%
%   The brace pattern's LOWERING is a function of the SOURCE COLUMN'S DECLARED
%   TYPE, never of the pattern.
%
%     rel resp(ep: text, body: json).       -- body: json  -> json1 plan
%       decode(body, {number: num})  ==>  json_extract(body, '$.number')
%
%     rel diag(where: place, message: text) -- where: place -> dictionary join
%       decode(where, {file: file})  ==>  '__dict_place'(where, file, _)
%
%   One surface, two lowerings, picked by the decl. Typed refs stay relational
%   (lower.pl expand_decode_rules/4, unchanged); `json` is the explicitly
%   dynamic escape and the only place the key axis is even meaningful, because
%   a declared struct HAS no unknown keys.
%
% STORAGE: json columns store TEXT with a `json_valid` CHECK. NOT jsonb --
% receipt_jsonb_is_not_portable below measures that the two SQLite instances
% this project already runs disagree about jsonb's existence.
%
% TICK-LOG CONTRACT: unchanged. canonical_json_text/2 (0_type_plane.pl) stays
% the writer's job; receipt_sqlite_does_not_canonicalize shows json1 will not
% do it for us at any point in the pipeline.

:- module(json_syntax_lowering,
          [ pattern_sql/4,
            lowering_receipts/1
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module('1_grammar', [parse_pattern/3]).

% ── pattern -> SQL ───────────────────────────────────────────────────────────
%
% pattern_sql(+SourceTable, +SourceColumn, +PatternIR, -Sql)
%
% State: st(NextAlias, FromClauses, WhereClauses, SelectPairs).

pattern_sql(Table, Column, Pattern, Sql) :-
    format(atom(RootExpr), 'b0."~w"', [Column]),
    compile_pattern(Pattern, RootExpr, '$',
                    st(0, [], [], []), st(_, Froms, Wheres, Selects0)),
    reverse(Selects0, Selects),
    (   Selects == []
    ->  SelectSql = '1'
    ;   findall(Text,
                ( member(Name-Expr, Selects),
                  format(atom(Text), '~w AS "~w"', [Expr, Name]) ),
                SelectTexts),
        atomic_list_concat(SelectTexts, ',\n       ', SelectSql)
    ),
    reverse(Froms, FromsOrdered),
    atomic_list_concat([Table, ' b0' | FromsOrdered], FromSql),
    reverse(Wheres, WheresOrdered),
    (   WheresOrdered == []
    ->  format(atom(Sql), 'SELECT ~w\n  FROM ~w;', [SelectSql, FromSql])
    ;   atomic_list_concat(WheresOrdered, '\n   AND ', WhereSql),
        format(atom(Sql), 'SELECT ~w\n  FROM ~w\n WHERE ~w;',
               [SelectSql, FromSql, WhereSql])
    ).

% An object pattern is OPEN and its exact-key members cost NO join: they walk
% the json path inside one json_extract, which is v5's "descent inside one
% match, not a join" (v3 walker.rs:1-8 "leaves join, descents fan out").
compile_pattern(pat_obj(Members), Expr, Path, State0, State) :- !,
    foldl(compile_member(Expr, Path), Members, State0, State).
compile_pattern(pat_arr_spread(Sub), Expr, Path, State0, State) :- !,
    each_alias(Expr, Path, Alias, State0, State1),
    subtree_guard(Sub, Alias, State1, State2),
    format(atom(ValueExpr), '~w.value', [Alias]),
    compile_pattern(Sub, ValueExpr, '$', State2, State).
compile_pattern(pat_arr_fixed(Items), Expr, Path, State0, State) :- !,
    foldl(compile_indexed_item(Expr, Path), Items, 0-State0, _-State).
compile_pattern(pat_hole(Name), Expr, Path, State0, State) :- !,
    path_expr(Expr, Path, ValueExpr),
    add_select(Name, ValueExpr, State0, State1),
    format(atom(NotNull), '~w IS NOT NULL', [ValueExpr]),
    add_where(NotNull, State1, State).
compile_pattern(pat_anon, Expr, Path, State0, State) :- !,
    path_expr(Expr, Path, ValueExpr),
    format(atom(NotNull), '~w IS NOT NULL', [ValueExpr]),
    add_where(NotNull, State0, State).
compile_pattern(pat_eq(Value), Expr, Path, State0, State) :- !,
    path_expr(Expr, Path, ValueExpr),
    sql_literal(Value, Literal),
    format(atom(Equality), '~w = ~w', [ValueExpr, Literal]),
    add_where(Equality, State0, State).
compile_pattern(pat_text(Template), _, _, _, _) :-
    % v5 parsed this and matched it LITERALLY (archive TASKS.md T7 never
    % shipped its semantics). A lowering that guessed would be inventing
    % semantics no generation ever had.
    throw(unsupported_construct(value_template_never_shipped(Template))).

compile_indexed_item(Expr, Path, Item, Index-State0, NextIndex-State) :-
    NextIndex is Index + 1,
    format(atom(ItemPath), '~w[~w]', [Path, Index]),
    compile_pattern(Item, Expr, ItemPath, State0, State).

% ── the key axis ─────────────────────────────────────────────────────────────
% Every key-axis matcher is ONE json_each (or json_tree) join. This is the
% whole cost of the five (d) rows of the recovery doc.

compile_member(Expr, Path, kp(k_exact(Key), Sub), State0, State) :- !,
    extend_path(Path, Key, SubPath),
    compile_pattern(Sub, Expr, SubPath, State0, State).
compile_member(Expr, Path, kp(k_hole(Name), Sub), State0, State) :- !,
    each_alias(Expr, Path, Alias, State0, State1),
    subtree_guard(Sub, Alias, State1, State2),
    format(atom(KeyExpr), '~w.key', [Alias]),
    add_select(Name, KeyExpr, State2, State3),
    format(atom(ValueExpr), '~w.value', [Alias]),
    compile_pattern(Sub, ValueExpr, '$', State3, State).
compile_member(Expr, Path, kp(k_anon, Sub), State0, State) :- !,
    each_alias(Expr, Path, Alias, State0, State1),
    subtree_guard(Sub, Alias, State1, State2),
    format(atom(ValueExpr), '~w.value', [Alias]),
    compile_pattern(Sub, ValueExpr, '$', State2, State).
compile_member(Expr, Path, kp(k_glob(Glob), Sub), State0, State) :- !,
    each_alias(Expr, Path, Alias, State0, State1),
    sql_literal(Glob, Literal),
    format(atom(Filter), '~w.key GLOB ~w', [Alias, Literal]),
    add_where(Filter, State1, State2),
    subtree_guard(Sub, Alias, State2, State3),
    format(atom(ValueExpr), '~w.value', [Alias]),
    compile_pattern(Sub, ValueExpr, '$', State3, State).
compile_member(Expr, Path, kp(k_re(Regex), Sub), State0, State) :- !,
    each_alias(Expr, Path, Alias, State0, State1),
    sql_literal(Regex, Literal),
    format(atom(Filter), '~w.key REGEXP ~w', [Alias, Literal]),
    add_where(Filter, State1, State2),
    subtree_guard(Sub, Alias, State2, State3),
    format(atom(ValueExpr), '~w.value', [Alias]),
    compile_pattern(Sub, ValueExpr, '$', State3, State).
compile_member(Expr, Path, kp(k_descend, Sub), State0, State) :- !,
    tree_alias(Expr, Path, Alias, State0, State1),
    subtree_guard(Sub, Alias, State1, State2),
    format(atom(ValueExpr), '~w.value', [Alias]),
    compile_pattern(Sub, ValueExpr, '$', State2, State).

% json_each/json_tree hand back SQL values, so `value` is the json TEXT for
% containers and a bare scalar for leaves. Descending into a leaf is not a
% silent non-match in SQLite -- json_extract raises "malformed JSON" and kills
% the whole statement. The emitted guard reads the table function's own `type`
% column, which is why a non-matching branch stays silent (v5's
% missing_key_yields_no_match, src/datapath.rs:1460-1464) instead of throwing.
subtree_guard(Sub, Alias, State0, State) :-
    (   sub_kind(Sub, Kind)
    ->  format(atom(Guard), '~w.type = \'~w\'', [Alias, Kind]),
        add_where(Guard, State0, State)
    ;   State = State0
    ).

sub_kind(pat_obj(_), object).
sub_kind(pat_arr_spread(_), array).
sub_kind(pat_arr_fixed(_), array).

% ── SQL fragment helpers ─────────────────────────────────────────────────────

each_alias(Expr, Path, Alias, st(N, Froms, Wheres, Selects),
           st(N1, [From | Froms], Wheres, Selects)) :-
    N1 is N + 1,
    format(atom(Alias), 'e~w', [N]),
    table_function('json_each', Expr, Path, Alias, From).

tree_alias(Expr, Path, Alias, st(N, Froms, Wheres, Selects),
           st(N1, [From | Froms], Wheres, Selects)) :-
    N1 is N + 1,
    format(atom(Alias), 't~w', [N]),
    table_function('json_tree', Expr, Path, Alias, From).

table_function(Function, Expr, '$', Alias, From) :- !,
    format(atom(From), ', ~w(~w) ~w', [Function, Expr, Alias]).
table_function(Function, Expr, Path, Alias, From) :-
    sql_literal(Path, Literal),
    format(atom(From), ', ~w(~w, ~w) ~w', [Function, Expr, Literal, Alias]).

path_expr(Expr, '$', Expr) :- !.
path_expr(Expr, Path, ValueExpr) :-
    sql_literal(Path, Literal),
    format(atom(ValueExpr), 'json_extract(~w, ~w)', [Expr, Literal]).

extend_path('$', Key, Path) :- !, format(atom(Path), '$.~w', [Key]).
extend_path(Prefix, Key, Path) :- format(atom(Path), '~w.~w', [Prefix, Key]).

add_select(Name, Expr, st(N, Froms, Wheres, Selects),
           st(N, Froms, Wheres, [Name-Expr | Selects])).

add_where(Condition, st(N, Froms, Wheres, Selects),
          st(N, Froms, [Condition | Wheres], Selects)).

sql_literal(Value, Literal) :-
    (   number(Value)
    ->  format(atom(Literal), '~w', [Value])
    ;   atom_string(Value, String),
        split_string(String, "'", "", Parts),
        atomic_list_concat(Parts, '''''', Escaped),
        format(atom(Literal), '''~w''', [Escaped])
    ).

% ── receipts ─────────────────────────────────────────────────────────────────

lowering_receipts(7) :-
    receipt_flat_and_nested_one_row,
    receipt_array_fanout_flagship,
    receipt_key_capture,
    receipt_recursive_descent_with_path,
    receipt_glob_and_regex_keys,
    receipt_jsonb_is_not_portable,
    receipt_sqlite_does_not_canonicalize.

% ── L1: exact keys, flat + nested, ONE ROW, ZERO joins ───────────────────────
% Recovery doc §1.3 / src/datapath.rs:1502-1509.
receipt_flat_and_nested_one_row :-
    parse_pattern(v5, '{ number: $n, user: { login: $a } }', Pattern),
    pattern_sql(resp, body, Pattern, Sql),
    Expected =
'SELECT json_extract(b0."body", \'$.number\') AS "n",
       json_extract(b0."body", \'$.user.login\') AS "a"
  FROM resp b0
 WHERE json_extract(b0."body", \'$.number\') IS NOT NULL
   AND json_extract(b0."body", \'$.user.login\') IS NOT NULL;',
    assert_sql_text(l1, Sql, Expected),
    Setup = 'CREATE TABLE resp(body TEXT NOT NULL CHECK(json_valid(body)));
INSERT INTO resp VALUES(\'{"number":7,"user":{"login":"alice"}}\');
INSERT INTO resp VALUES(\'{"number":8}\');',
    sqlite_rows(Setup, Sql, Rows),
    Rows == ["7|alice"],
    format("PASS L1 flat+nested exact keys: 0 joins, 1 row, missing key silently drops~n").

% ── L2: THE FLAGSHIP. Array-of-objects fan-out with sibling correlation ──────
% examples/gh-cache.dl:114-124, the recovery doc's flagship acceptance case.
receipt_array_fanout_flagship :-
    parse_pattern(v5,
        '[... { number: $num, title: $title, state: $state, user: { login: $author } } ]',
        Pattern),
    pattern_sql(resp, body, Pattern, Sql),
    Expected =
'SELECT json_extract(e0.value, \'$.number\') AS "num",
       json_extract(e0.value, \'$.title\') AS "title",
       json_extract(e0.value, \'$.state\') AS "state",
       json_extract(e0.value, \'$.user.login\') AS "author"
  FROM resp b0, json_each(b0."body") e0
 WHERE e0.type = \'object\'
   AND json_extract(e0.value, \'$.number\') IS NOT NULL
   AND json_extract(e0.value, \'$.title\') IS NOT NULL
   AND json_extract(e0.value, \'$.state\') IS NOT NULL
   AND json_extract(e0.value, \'$.user.login\') IS NOT NULL;',
    assert_sql_text(l2, Sql, Expected),
    Setup = 'CREATE TABLE resp(body TEXT NOT NULL CHECK(json_valid(body)));
INSERT INTO resp VALUES(\'[{"number":1,"title":"t1","state":"open","user":{"login":"a"}},
 {"number":2,"title":"t2","state":"closed","user":{"login":"b"}}]\');',
    sqlite_rows(Setup, Sql, Rows),
    Rows == ["1|t1|open|a", "2|t2|closed|b"],
    format("PASS L2 flagship array fan-out: 1 join, 1 row per element, siblings correlated~n").

% ── L3: KEY CAPTURE, the single highest-leverage (d) row ─────────────────────
% examples/type-from-json.dl:25. json_each already yields (key, value): the
% construct the recovery doc graded "no v6 spelling" needs NO new SQL at all.
receipt_key_capture :-
    parse_pattern(v5, '{ $key: $value }', Pattern),
    pattern_sql(sample, body, Pattern, Sql),
    Expected =
'SELECT e0.key AS "key",
       e0.value AS "value"
  FROM sample b0, json_each(b0."body") e0
 WHERE e0.value IS NOT NULL;',
    assert_sql_text(l3, Sql, Expected),
    Setup = 'CREATE TABLE sample(body TEXT NOT NULL CHECK(json_valid(body)));
INSERT INTO sample VALUES(\'{"name":"cli","stars":4}\');',
    sqlite_rows(Setup, Sql, Rows),
    Rows == ["name|cli", "stars|4"],
    format("PASS L3 key capture = json_each(key,value); zero new SQL machinery~n").

% ── L4: `**` recursive descent, AND the path v4 wanted and v5 dropped ────────
% src/datapath.rs:1487-1494 for `**`; v4's `$$${PATH?}` is json_tree.fullkey,
% which the recovery doc lists as dropped-with-no-successor.
receipt_recursive_descent_with_path :-
    parse_pattern(v5, '{ **: { image: $i } }', Pattern),
    pattern_sql(doc, body, Pattern, Sql),
    Expected =
'SELECT json_extract(t0.value, \'$.image\') AS "i"
  FROM doc b0, json_tree(b0."body") t0
 WHERE t0.type = \'object\'
   AND json_extract(t0.value, \'$.image\') IS NOT NULL;',
    assert_sql_text(l4, Sql, Expected),
    Setup = 'CREATE TABLE doc(body TEXT NOT NULL CHECK(json_valid(body)));
INSERT INTO doc VALUES(\'{"a":{"b":{"image":"deep"}},"top":1}\');',
    sqlite_rows(Setup, Sql, Rows),
    Rows == ["deep"],
    % The same join already carries the traversed path, for free.
    PathSql = 'SELECT json_extract(t0.value, \'$.image\') AS "i", t0.fullkey AS "path"
  FROM doc b0, json_tree(b0."body") t0
 WHERE t0.type = \'object\'
   AND json_extract(t0.value, \'$.image\') IS NOT NULL;',
    sqlite_rows(Setup, PathSql, PathRows),
    PathRows == ["deep|$.a.b"],
    format("PASS L4 `**` = json_tree; v4's dropped path capture is fullkey, free~n").

% ── L5: glob keys are free; regex keys are NOT core SQLite ───────────────────
receipt_glob_and_regex_keys :-
    parse_pattern(v5, '{ *id: $v }', GlobPattern),
    pattern_sql(doc, body, GlobPattern, GlobSql),
    Expected =
'SELECT e0.value AS "v"
  FROM doc b0, json_each(b0."body") e0
 WHERE e0.key GLOB \'*id\'
   AND e0.value IS NOT NULL;',
    assert_sql_text(l5, GlobSql, Expected),
    Setup = 'CREATE TABLE doc(body TEXT NOT NULL CHECK(json_valid(body)));
INSERT INTO doc VALUES(\'{"uid":"a","gid":"b","name":"c"}\');',
    sqlite_rows(Setup, GlobSql, GlobRows),
    GlobRows == ["a", "b"],
    parse_pattern(v5, '{ re:^v: $val }', RePattern),
    pattern_sql(doc, body, RePattern, ReSql),
    sub_atom(ReSql, _, _, _, 'e0.key REGEXP \'^v\''),
    % REGEXP is syntax in core SQLite with NO implementation; the CLI and
    % @libsql each supply one, rusqlite by default does not. Measured here so
    % the rust-flip directive prices the `re:` key honestly.
    ReSetup = 'CREATE TABLE doc(body TEXT NOT NULL CHECK(json_valid(body)));
INSERT INTO doc VALUES(\'{"v1":"x","v2":"y","other":"z"}\');',
    sqlite_rows(ReSetup, ReSql, ReRows),
    ReRows == ["x", "y"],
    format("PASS L5 glob key = SQL GLOB (core); regex key needs a non-core REGEXP~n").

% ── L6: jsonb is NOT portable across the two SQLite builds we already run ────
% System sqlite3 here is 3.43.2 (no jsonb, added in 3.45). @libsql/client
% bundles 3.45.1 (jsonb present, measured out-of-band and recorded in the plan
% doc). A storage decision cannot depend on a function only one of them has.
receipt_jsonb_is_not_portable :-
    sqlite_scalar('SELECT sqlite_version();', Version),
    sqlite_error('SELECT typeof(jsonb(\'{"a":1}\'));', ErrorText),
    sub_string(ErrorText, _, _, _, "jsonb"),
    format("PASS L6 system sqlite ~w rejects jsonb -> json columns store TEXT~n",
           [Version]).

% ── L7: json1 will not canonicalize for us; the writer keeps that job ────────
% The cross-target tick-log contract (ruling json_ticklog_encoding) is sorted
% keys, no whitespace. json() minifies but PRESERVES key order, and
% json_group_object follows row order. So canonical_json_text/2 stays the
% single canonicalizer and nothing about the contract moves.
receipt_sqlite_does_not_canonicalize :-
    sqlite_scalar('SELECT json(\'{"b":1,"a":2}\');', Preserved),
    Preserved == "{\"b\":1,\"a\":2}",
    sqlite_scalar('SELECT json(\'{"a":2,"b":1}\');', Canonical),
    Canonical == "{\"a\":2,\"b\":1}",
    % Idempotent on already-canonical text: storing canonical text and reading
    % it back through json1 is a no-op, which is what keeps the log stable.
    sqlite_scalar('SELECT json(json(\'{"a":2,"b":1}\'));', Canonical),
    % Explicit ORDER BY is what buys canonical order at the SQL boundary.
    sqlite_scalar(
        'WITH t(k,v) AS (VALUES(\'b\',1),(\'a\',2)) SELECT json_group_object(k,v) FROM (SELECT * FROM t ORDER BY k);',
        "{\"a\":2,\"b\":1}"),
    % ... and without it, json_group_object follows row order, not key order.
    sqlite_scalar(
        'WITH t(k,v) AS (VALUES(\'b\',1),(\'a\',2)) SELECT json_group_object(k,v) FROM t;',
        "{\"b\":1,\"a\":2}"),
    format("PASS L7 json1 preserves key order; canonicalization stays the writer's job~n").

% ── sqlite plumbing ──────────────────────────────────────────────────────────

assert_sql_text(Id, Actual, Expected) :-
    (   Actual == Expected
    ->  true
    ;   format("~n--- ~w EMITTED ---~n~w~n--- EXPECTED ---~n~w~n", [Id, Actual, Expected]),
        throw(sql_text_mismatch(Id))
    ).

sqlite_rows(Setup, Sql, Rows) :-
    atomic_list_concat([Setup, '\n', Sql], Script),
    sqlite_command([':memory:', Script], Text),
    split_string(Text, "\n", "", Raw),
    exclude(==(""), Raw, Rows).

sqlite_scalar(Sql, Value) :-
    sqlite_command([':memory:', Sql], Text),
    split_string(Text, "\n", "", Raw),
    exclude(==(""), Raw, [Value | _]).

sqlite_error(Sql, ErrorText) :-
    process_create('/usr/bin/sqlite3', [':memory:', Sql],
                   [stdout(pipe(Out)), stderr(pipe(Err)), process(Pid)]),
    read_string(Out, _, _),
    read_string(Err, _, ErrorText),
    close(Out), close(Err),
    process_wait(Pid, _),
    ErrorText \== "".

sqlite_command(Arguments, Text) :-
    process_create('/usr/bin/sqlite3', Arguments,
                   [stdout(pipe(Out)), stderr(pipe(Err)), process(Pid)]),
    read_string(Out, _, Text),
    read_string(Err, _, ErrorText),
    close(Out), close(Err),
    process_wait(Pid, Status),
    (   Status == exit(0), ErrorText == ""
    ->  true
    ;   throw(sqlite_failed(Arguments, Status, ErrorText))
    ).
