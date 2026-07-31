% JSON interop receipts against the current V6 oracle, compiler, and SQLite.
%
% Run:
%   swipl -q -l v6/prolog/labs/json_interop/0_receipts.pl -g go -g halt
%
% This file introduces no DL spelling. Terms used below are current internal
% IR, current declarations, or SQL sent directly to SQLite.

:- module(json_interop_receipts, [go/0]).

:- use_module('../../conformance/body', [json_canon/2]).
:- use_module('../../conformance/engine',
              [run_program/5, rel_rows/3, rel_deltas/3]).
:- use_module('../../0_enum_expand', [expand_enum_program/2]).
:- use_module('../../0_type_plane',
              [type_definitions/2, column_storage/3,
               type_cycle_witness/2, canonical_json_text/2]).
:- use_module('../../1_host_expand', [prepare_program/5]).
:- use_module('../../compile', [program_plan/2]).
:- use_module('../../lower', [lower_program/2]).
:- use_module('../../compile/parse_dl', [parse_dl/4]).
:- use_module(library(process)).
:- use_module(library(readutil), [read_file_to_string/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700, xfx, :=).

go :-
    receipt_relational_object_and_array,
    receipt_no_null,
    receipt_enum_variants_are_relations,
    receipt_relation_reference_storage,
    receipt_host_boundary_keeps_declared_shape,
    receipt_sqlite_json1_interop,
    receipt_json_clock_and_retraction,
    receipt_recursive_value_boundary,
    receipt_storage_amplification,
    receipt_optional_module_current_absence,
    receipt_schema_import_current_absence,
    receipt_opt_out_storage,
    format("12 PASS~n").

receipt_relational_object_and_array :-
    Source =
        obj([items-[obj([name-beta]), obj([name-alpha])],
             owner-obj([login-octo])]),
    Program =
        prog(
            [],
            [ (item(Item) <-
                  raw(SourceValue),
                  decode(SourceValue, {items: Items}),
                  json_each(Items, Item)),
              (name(Name) <- item(Item), decode(Item, {name: Name})),
              (names(json_array(Name)) <- name(Name))
            ]),
    run_program(Program, [raw(Source)], [], Final, _),
    rel_rows(item/1, Final,
             [item(obj([name-alpha])), item(obj([name-beta]))]),
    rel_rows(name/1, Final, [name(alpha), name(beta)]),
    rel_rows(names/1, Final, [names([alpha, beta])]),
    json_canon({owner: {login: octo}, items: [1, 2]},
               obj([items-[1, 2], owner-obj([login-octo])])),
    format("PASS current oracle relates object fields and array elements as rows~n").

receipt_no_null :-
    canonical_json_text(none, '"none"'),
    canonical_json_text(null, '"null"'),
    Program =
        prog(
            [],
            [ (present(Value) <-
                  raw(Body),
                  decode(Body, {value: Value}))
            ]),
    run_program(Program, [raw(obj([value-none]))], [], Final, _),
    rel_rows(present/1, Final, []),
    format("PASS none/null atoms render as strings; bare decode treats none as absence~n").

receipt_enum_variants_are_relations :-
    Surface =
        prog(
            [enum_decl(result,
                       (ok(value:text) ; error(message:text)))],
            []),
    expand_enum_program(Surface, prog(Decls, Rules)),
    member(col_type(result_ok/2, id, int), Decls),
    member(col_type(result_ok/2, value, text), Decls),
    member(col_type(result_error/2, id, int), Decls),
    member(col_type(result_error/2, message, text), Decls),
    member((result_tag(Id, ok) <- result_ok(Id, _)), Rules),
    member((result_tag(Id, error) <- result_error(Id, _)), Rules),
    \+ member(enum_decl(_, _), Decls),
    format("PASS enum expands to variant relations plus a derived tag relation~n").

receipt_relation_reference_storage :-
    Program =
        prog(
            [ type_decl(span, [col(start, int), col(end, int)]),
              col_type(mark/1, at, span)
            ],
            []),
    type_definitions(
        [type_decl(span, [col(start, int), col(end, int)])],
        Types),
    column_storage(Types, span, ref(span)),
    program_plan(fixture(json_ref_storage, Program, [], [], [])-[], Plan),
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    member(ParentDdl, Ddl),
    sub_string(ParentDdl, _, _, _,
               "CREATE TABLE \"mark\" (\"at\" INTEGER NOT NULL"),
    member(ValueDdl, Ddl),
    sub_string(ValueDdl, _, _, _,
               "CREATE TABLE \"span\" (\"__id\" INTEGER PRIMARY KEY"),
    member(RenderView, Ddl),
    sub_string(RenderView, _, _, _, "CREATE TEMP VIEW \"__ref_span\""),
    sub_string(RenderView, _, _, _, "json_object('start'"),
    format("PASS declared relation reference lowers to dense id plus relational value row~n").

receipt_host_boundary_keeps_declared_shape :-
    Surface =
        program(
            [ type_decl(span, [col(start, int), col(end, int)]),
              col_type(source_path/1, path, text),
              col_type(host_span/2, path, text),
              col_type(host_span/2, at, span),
              sh_decl(scan_json,
                      [col(path, text)],
                      [col(at, span)],
                      template("scan {path}"))
            ],
            [ (host_span(Path, At) <-
                  source_path(Path),
                  probe(scan_json, [Path], [At], []))
            ],
            []),
    prepare_program(Surface, prog(Decls, _), HostPlans, [], []),
    member(host_plan(scan_json,
                     [col(path, text)],
                     [col(at, span)],
                     template("scan {path}"),
                     demand_ref('__host_demand_scan_json'),
                     response_ref('__host_response_scan_json'),
                     _),
           HostPlans),
    ResponseRef = '__host_response_scan_json'/4,
    member(col_type(ResponseRef, at, span), Decls),
    member(keyed(ResponseRef, [1, 2]), Decls),
    format("PASS host output keeps the declared ref type on its response relation~n").

receipt_sqlite_json1_interop :-
    sqlite_scalar(
        "WITH t(k,v) AS (VALUES('b',2),('a',1)) SELECT json(json_group_object(k,v)) FROM (SELECT * FROM t ORDER BY k);",
        '{"a":1,"b":2}'),
    sqlite_scalar(
        "SELECT json_extract('{\"a\":[1,2]}','$.a[1]');",
        '2'),
    sqlite_scalar(
        "SELECT value FROM json_each('[\"x\",\"y\"]') ORDER BY key;",
        "x\ny"),
    format("PASS system SQLite json1 constructs, extracts, and explodes canonical values~n").

receipt_json_clock_and_retraction :-
    Program =
        prog(
            [],
            [ (document(json_object(Key, Value)) <- kv(Key, Value))
            ]),
    Schedule =
        [ [+kv(name, cli), +kv(stars, 4)],
          [-kv(stars, 4)]
        ],
    run_program(Program, [], Schedule, Final, Deltas),
    rel_deltas(
        document/1,
        Deltas,
        [ [+document(obj([name-cli, stars-4]))],
          [-document(obj([name-cli, stars-4])),
           +document(obj([name-cli]))]
        ]),
    rel_rows(document/1, Final, [document(obj([name-cli]))]),
    format("PASS JSON aggregate replacement and retraction use ordinary tick deltas~n").

receipt_recursive_value_boundary :-
    Value =
        obj([children-[obj([children-[], name-leaf])],
             name-root]),
    canonical_json_text(
        Value,
        '{"children":[{"children":[],"name":"leaf"}],"name":"root"}'),
    RecursiveTypes =
        [type_def(node, [next], [node])],
    type_cycle_witness(RecursiveTypes, [node]),
    catch(column_storage([], list(text), _),
          unsupported_construct(column_type_unknown(list(text))),
          ListRefused = true),
    ListRefused == true,
    format("PASS finite recursive JSON renders; cyclic value refs and typed lists remain refused~n").

receipt_storage_amplification :-
    atomics_to_string(
        [ "CREATE TABLE parent(id INTEGER PRIMARY KEY,payload TEXT NOT NULL); ",
          "WITH RECURSIVE n(x) AS ",
          "(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) ",
          "INSERT INTO parent ",
          "SELECT x,json_object(",
          "'path','src/really/duplicative/name/file.ts',",
          "'start',10,'end',20,'kind','node') FROM n; ",
          "VACUUM; ",
          "SELECT page_count*page_size ",
          "FROM pragma_page_count(), pragma_page_size();"
        ],
        EmbeddedSql),
    atomics_to_string(
        [ "CREATE TABLE span(",
          "id INTEGER PRIMARY KEY,path TEXT NOT NULL,start INTEGER NOT NULL,",
          "end INTEGER NOT NULL,kind TEXT NOT NULL,",
          "UNIQUE(path,start,end,kind)); ",
          "INSERT INTO span VALUES(",
          "1,'src/really/duplicative/name/file.ts',10,20,'node'); ",
          "CREATE TABLE parent(id INTEGER PRIMARY KEY,span_id INTEGER NOT NULL); ",
          "WITH RECURSIVE n(x) AS ",
          "(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) ",
          "INSERT INTO parent SELECT x,1 FROM n; ",
          "VACUUM; ",
          "SELECT page_count*page_size ",
          "FROM pragma_page_count(), pragma_page_size();"
        ],
        RelationalSql),
    sqlite_file_bytes(EmbeddedSql, EmbeddedBytes),
    sqlite_file_bytes(RelationalSql, RelationalBytes),
    EmbeddedBytes > RelationalBytes,
    Ratio is EmbeddedBytes / RelationalBytes,
    Ratio > 5,
    format("PASS repeated JSON ~d bytes; relational ref ~d bytes; ratio ~2f~n",
           [EmbeddedBytes, RelationalBytes, Ratio]).

receipt_optional_module_current_absence :-
    string_codes("use \"std/entry.dl\".", Codes),
    catch(parse_dl(Codes, _, _, _), dl_parse_error(_, _), Refused = true),
    Refused == true,
    Program =
        prog([], [(value(Value) <- raw(Body), decode(Body, {x: Value}))]),
    run_program(Program, [raw(obj([x-1]))], [], Final, _),
    rel_rows(value/1, Final, [value(1)]),
    format("PASS V6 has no module-import surface; JSON oracle semantics are global~n").

receipt_schema_import_current_absence :-
    read_file_to_string('v6/prolog/ARCH.pl', Arch, []),
    split_string(Arch, "\n", "", Lines),
    member(Line, Lines),
    sub_string(Line, _, _, _, "task(schema_import_epic"),
    sub_string(Line, _, _, _, "unbuilt"),
    read_file_to_string('v6/prolog/compile/parse_dl.pl', Parser, []),
    \+ sub_string(Parser, _, _, _, "schema_import("),
    format("PASS schema import is recorded unbuilt and has no current parser production~n").

receipt_opt_out_storage :-
    Plain =
        prog(
            [col_type(event/1, value, int)],
            []),
    program_plan(
        fixture(json_opt_out_plain, Plain, [], [[+event(1)]], [])-[],
        Plan),
    lower_program(Plan, lowered(_, Ddl, _, _, _, _, _, _)),
    \+ (member(Sql, Ddl), sub_string(Sql, _, _, _, "__ref_")),
    \+ (member(Sql, Ddl), sub_string(Sql, _, _, _, "json_object(")),
    catch(
        program_plan(
            fixture(json_aggregate_current_refusal,
                    prog([], [(bag(json_array(Value)) <- item(Value))]),
                    [], [], [])-[],
            _),
        unsupported_construct(aggregate_head(_)),
        AggregateRefused = true),
    AggregateRefused == true,
    format("PASS non-JSON program allocates no reference target; JSON aggregate stays named refusal~n").

sqlite_scalar(Sql, Expected) :-
    sqlite_command([':memory:', Sql], Text),
    normalize_space(string(Normalized), Text),
    normalize_space(string(ExpectedString), Expected),
    Normalized == ExpectedString.

sqlite_file_bytes(Sql, Bytes) :-
    tmp_file(json_interop, Path),
    setup_call_cleanup(
        true,
        ( sqlite_command([Path, Sql], Text),
          normalize_space(string(Normalized), Text),
          number_string(Bytes, Normalized)
        ),
        ( exists_file(Path) -> delete_file(Path) ; true )).

sqlite_command(Arguments, Text) :-
    process_create('/usr/bin/sqlite3', Arguments,
                   [stdout(pipe(Out)), stderr(pipe(Err)), process(Pid)]),
    read_string(Out, _, Text),
    read_string(Err, _, ErrorText),
    close(Out),
    close(Err),
    process_wait(Pid, Status),
    ( Status == exit(0)
    -> true
    ; throw(sqlite_failed(Status, ErrorText))
    ).
