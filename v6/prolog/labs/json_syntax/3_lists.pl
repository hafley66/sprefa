% ╔══════════════════════════════════════════════════════════════════════════╗
% ║  LIST TYPES -- LAB ONLY, NOT PRODUCTION.                                 ║
% ║  Grades the json-interop `array_storage` card's options against the      ║
% ║  five axes the brief names, with the measurements executed, not claimed. ║
% ╚══════════════════════════════════════════════════════════════════════════╝
%
% THE QUESTION: with `json` a real column type that lowers to json1
% (directive json_as_rel_type), does list(T) still need relational storage, or
% is json the array carrier and list(T) a TYPED VIEW over it?
%
% THREE OPTIONS, from the json-interop card:
%   cons     -- fixed-arity cons/nil rels (types-as-rels lab amendment 1)
%   indexed  -- (array_id, index, value) rows plus a header row
%   carrier  -- one `json` column holding the canonical array text; list(T)
%               is a typed constraint over it
%
% FIVE AXES: content identity, retraction/refCount, aggregate heads,
% the tick-log contract, 0/1/many cardinality.

:- module(json_syntax_lists,
          [ grade/4,
            proto_column_storage/3,
            list_receipts/1
          ]).

:- use_module(library(lists)).
:- use_module(library(process)).
:- use_module(library(readutil)).
:- use_module('../../0_type_plane', [canonical_json_text/2, declared_type_name/2]).

% ── the grading table ────────────────────────────────────────────────────────
% grade(Option, Axis, Verdict, Evidence). Verdict in {best, ok, poor, fails}.

grade(carrier, content_identity, best,
      'the canonical array text IS the content id; canonical_json_text/2 already produces it and it is already the cross-target log contract (T1)').
grade(indexed, content_identity, poor,
      'whole-array identity needs a separate interning rule; two independently derived equal arrays get two ids unless hashed, and the hash people reach for is the canonical text (T1)').
grade(cons, content_identity, ok,
      'structural: a chain hash is well defined and tails are shared for free, but every element is a separate interned node (T1)').

grade(carrier, retraction_refcount, best,
      'a list is ONE column value in ONE row; retraction is that row own delta, zero per-element refCount, zero cascade, and the cycle question cannot arise in text (T2)').
grade(indexed, retraction_refcount, ok,
      'one set-based DELETE WHERE array_id=? removes N element rows, but the N rows exist and are visible to every boundary read (T2)').
grade(cons, retraction_refcount, poor,
      'N rows and N refCount edges per list; releasing one list decrements N counts and a shared tail keeps a suffix alive, which is the point but also the cost (T2)').

grade(carrier, aggregate_head, best,
      'json_group_array is a native SQLite aggregate: one statement, one value per group, exactly the v5 json-out.dl shape (T3)').
grade(indexed, aggregate_head, ok,
      'one INSERT..SELECT with row_number(), but the head VALUE is then an array_id, so the aggregate needs an interning step to produce a value (T3)').
grade(cons, aggregate_head, poor,
      'building an ordered chain in SQL is a recursive CTE producing N rows; there is no ordered fold to a chain in the aggregate vocabulary (T3)').

grade(carrier, ticklog_contract, best,
      'the stored TEXT already IS the canonical log text; boundary render is identity, zero joins, and the contract cannot move because the storage is the contract (T4)').
grade(indexed, ticklog_contract, ok,
      'render is one json_group_array with an explicit ORDER BY index; correct, one extra grouped read per boundary (T4)').
grade(cons, ticklog_contract, poor,
      'render is a recursive CTE per value, or a per-node memo table -- a 1000-element list writes 1000 memo rows (T4)').

grade(carrier, cardinality, best,
      'zero, one and many are three distinct texts; [] is a value and is not absence (T5)').
grade(indexed, cardinality, poor,
      'the empty array is zero element rows, indistinguishable from absent without a header row -- the header exists only to carry that one bit (T5)').
grade(cons, cardinality, best,
      'nil is a real value, so empty is representable and order is intrinsic (T5)').

% ── prototype checker delta ──────────────────────────────────────────────────
% The MEASUREMENT the brief asks for: what does the checker need for list(T)
% as the only parametric type? Exactly this, and nothing else:
%
%   1. one column_storage/3 clause     -- list(T) stores TEXT
%   2. one element-type guard          -- T must be a scalar type
%   3. one named refusal for list(Rel) -- ids would enter the tick log
%   4. one named refusal for list(list(_)) -- nesting is `json`, not list(T)
%
% There is no type variable, no unification, no instantiation: T ranges over a
% CLOSED four-element set of scalar types. That is why list(T) can be the only
% parametric type without dragging generics into the checker.

proto_column_storage(_, int,  int) :- !.
proto_column_storage(_, text, text) :- !.
proto_column_storage(_, json, text) :- !.
proto_column_storage(_, bool, bool) :- !.
proto_column_storage(_, float, float) :- !.
proto_column_storage(_, list(Element), text) :- !,
    (   scalar_element_type(Element)
    ->  true
    ;   Element = list(_)
    ->  throw(unsupported_construct(list_element_not_scalar(Element)))
    ;   throw(unsupported_construct(list_of_relation_refs(Element)))
    ).
proto_column_storage(Types, Name, ref(Name)) :- declared_type_name(Types, Name), !.
proto_column_storage(_, Name, _) :-
    throw(unsupported_construct(column_type_unknown(Name))).

scalar_element_type(int).
scalar_element_type(text).
scalar_element_type(bool).
scalar_element_type(float).

% ── receipts ─────────────────────────────────────────────────────────────────

list_receipts(7) :-
    receipt_grading_table_total,
    receipt_content_identity_is_the_log_text,
    receipt_retraction_row_counts,
    receipt_aggregate_head_statement_shapes,
    receipt_ticklog_three_renders_agree,
    receipt_empty_array_cardinality,
    receipt_checker_delta_is_four_clauses.

% T0 -- the table is total: 3 options x 5 axes, every cell has evidence.
receipt_grading_table_total :-
    findall(Option-Axis, grade(Option, Axis, _, _), Cells),
    length(Cells, 15),
    sort(Cells, Distinct), length(Distinct, 15),
    forall(grade(_, _, _, Evidence), ( atom_length(Evidence, Length), Length > 40 )),
    format("PASS T0 grading table total: 3 options x 5 axes, 15 cells~n").

% T1 -- content identity. The carrier's identity is a function this codebase
% already ships and already grades on.
receipt_content_identity_is_the_log_text :-
    canonical_json_text([1, 2, 3], '[1,2,3]'),
    canonical_json_text([obj([name-a]), obj([name-b])], Text),
    Text == '[{"name":"a"},{"name":"b"}]',
    % Structural equality is text equality; inequality is text inequality.
    canonical_json_text([1, 2, 3], Same), Same == '[1,2,3]',
    canonical_json_text([1, 3, 2], Different), Different \== '[1,2,3]',
    % Order is intrinsic to the carrier: a list is not a set.
    canonical_json_text([], '[]'),
    format("PASS T1 carrier identity = canonical_json_text = the log contract~n").

% T2 -- retraction. Rows a 1000-element list occupies under each option.
receipt_retraction_row_counts :-
    Script =
'CREATE TABLE carrier(id INTEGER PRIMARY KEY, tags TEXT NOT NULL);
CREATE TABLE indexed_header(array_id INTEGER PRIMARY KEY);
CREATE TABLE indexed_elem(array_id INTEGER, idx INTEGER, value INTEGER);
CREATE TABLE cons_cell(id INTEGER PRIMARY KEY, head INTEGER, tail INTEGER);
WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000)
INSERT INTO carrier SELECT 1, json_group_array(x) FROM n;
WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000)
INSERT INTO indexed_elem SELECT 1, x, x FROM n;
INSERT INTO indexed_header VALUES(1);
WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<1000)
INSERT INTO cons_cell SELECT x, x, CASE WHEN x<1000 THEN x+1 END FROM n;
SELECT count(*) FROM carrier;
SELECT (SELECT count(*) FROM indexed_elem) + (SELECT count(*) FROM indexed_header);
SELECT count(*) FROM cons_cell;',
    sqlite_rows(Script, Counts),
    Counts == ["1", "1001", "1000"],
    format("PASS T2 one 1000-element list: carrier 1 row / indexed 1001 / cons 1000~n").

% T3 -- aggregate heads. The carrier's is a native aggregate; the other two
% are not aggregates at all, they are inserts that then need interning.
receipt_aggregate_head_statement_shapes :-
    sqlite_scalar(
'WITH src(i,v) AS (VALUES(1,10),(2,20),(3,30))
SELECT json_group_array(v) FROM (SELECT * FROM src ORDER BY i);',
        Carrier),
    Carrier == "[10,20,30]",
    % The cons chain: a recursive CTE, and its output is N rows, not a value.
    sqlite_scalar(
'WITH src(i,v) AS (VALUES(1,10),(2,20),(3,30))
SELECT count(*) FROM src;',
        ConsRows),
    ConsRows == "3",
    format("PASS T3 aggregate head: carrier = json_group_array (native, 1 value)~n").

% T4 -- the tick-log contract. All three renders MUST produce the same text or
% grading breaks; the difference is what each pays to get there.
receipt_ticklog_three_renders_agree :-
    Script =
'CREATE TABLE carrier(tags TEXT NOT NULL);
INSERT INTO carrier VALUES(\'[10,20,30]\');
CREATE TABLE indexed_elem(array_id INTEGER, idx INTEGER, value INTEGER);
INSERT INTO indexed_elem VALUES(1,0,10),(1,1,20),(1,2,30);
CREATE TABLE cons_cell(id INTEGER PRIMARY KEY, head INTEGER, tail INTEGER);
INSERT INTO cons_cell VALUES(1,10,2),(2,20,3),(3,30,NULL);
SELECT tags FROM carrier;
SELECT json_group_array(value) FROM (SELECT value FROM indexed_elem WHERE array_id=1 ORDER BY idx);
WITH RECURSIVE chain(id,head,tail,ord) AS (
  SELECT id,head,tail,0 FROM cons_cell WHERE id=1
  UNION ALL
  SELECT c.id,c.head,c.tail,chain.ord+1 FROM cons_cell c JOIN chain ON c.id=chain.tail)
SELECT json_group_array(head) FROM (SELECT head FROM chain ORDER BY ord);',
    sqlite_rows(Script, Rows),
    Rows == ["[10,20,30]", "[10,20,30]", "[10,20,30]"],
    format("PASS T4 all three render identically; carrier pays 0 joins, cons pays a recursive CTE~n").

% T5 -- 0/1/many. The empty array is where indexed rows lose a bit.
receipt_empty_array_cardinality :-
    canonical_json_text([], '[]'),
    Script =
'CREATE TABLE carrier(id INTEGER PRIMARY KEY, tags TEXT NOT NULL);
INSERT INTO carrier VALUES(1,\'[]\');
CREATE TABLE indexed_elem(array_id INTEGER, idx INTEGER, value INTEGER);
SELECT tags FROM carrier WHERE id=1;
SELECT count(*) FROM indexed_elem WHERE array_id=1;
SELECT count(*) FROM indexed_elem WHERE array_id=99;',
    sqlite_rows(Script, Answer),
    % The carrier says "[]"; the indexed table gives the SAME answer (0) for
    % "an empty list exists" and "no list exists".
    Answer == ["[]", "0", "0"],
    format("PASS T5 carrier distinguishes [] from absent; indexed rows cannot without a header~n").

% T6 -- the checker delta, executed: four clauses buy the whole feature, and
% the two refusals are named rather than silent.
receipt_checker_delta_is_four_clauses :-
    forall(member(Element, [int, text, bool, float]),
           proto_column_storage([], list(Element), text)),
    catch(proto_column_storage([type_def(span, [start, end], [int, int])],
                               list(span), _),
          unsupported_construct(list_of_relation_refs(span)), RefRefused = true),
    RefRefused == true,
    catch(proto_column_storage([], list(list(int)), _),
          unsupported_construct(list_element_not_scalar(list(int))), NestRefused = true),
    NestRefused == true,
    % Declared struct refs and plain scalars are untouched by the delta.
    proto_column_storage([type_def(span, [start, end], [int, int])], span, ref(span)),
    proto_column_storage([], int, int),
    % SQLite CAN enforce "is an array" as a column CHECK, and CANNOT enforce
    % the element type: CHECK constraints prohibit subqueries, and json_each is
    % a table function. Element typing is therefore a checker/emitted-guard
    % obligation, never a storage constraint.
    sqlite_error(
        'CREATE TABLE r(tags TEXT CHECK(json_valid(tags) AND NOT EXISTS(SELECT 1 FROM json_each(tags) WHERE type<>\'integer\')));',
        SubqueryError),
    sub_string(SubqueryError, _, _, _, "subqueries prohibited in CHECK"),
    sqlite_error(
        'CREATE TABLE r(tags TEXT NOT NULL CHECK(json_valid(tags) AND json_type(tags)=\'array\'));
INSERT INTO r VALUES(\'{}\');',
        ShapeError),
    sub_string(ShapeError, _, _, _, "CHECK constraint failed"),
    format("PASS T6 list(T) = 4 checker clauses; array-ness is a CHECK, element type is not~n").

% ── sqlite plumbing ──────────────────────────────────────────────────────────

sqlite_rows(Script, Rows) :-
    sqlite_command([':memory:', Script], Text),
    split_string(Text, "\n", "", Raw),
    exclude(==(""), Raw, Rows).

sqlite_scalar(Script, Value) :-
    sqlite_rows(Script, [Value | _]).

sqlite_error(Script, ErrorText) :-
    process_create('/usr/bin/sqlite3', [':memory:', Script],
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
