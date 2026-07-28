:- use_module(library(process)).
:- use_module(library(readutil)).

:- prolog_load_context(directory, LabDirectory),
   assertz(sqlite_udf_lab_directory(LabDirectory)).

expected_registration("regexp").
expected_registration("sprf_split").
expected_registration("sprf_sym_intern").
expected_registration("sprf_lower").
expected_registration("sprf_upper").
expected_registration("sprf_lcfirst").
expected_registration("sprf_ucfirst").
expected_registration("sprf_trim").
expected_registration("sprf_norm").
expected_registration("sprf_strip_prefix").
expected_registration("sprf_strip_suffix").
expected_registration("sprf_sym").
expected_registration("sprf_lines").
expected_registration("sprf_replace_re").

lab_file(Name, Path) :-
    sqlite_udf_lab_directory(LabDirectory),
    directory_file_path(LabDirectory, Name, Path).

repo_file(Name, Path) :-
    sqlite_udf_lab_directory(LabDirectory),
    directory_file_path(LabDirectory, '..', LabsDirectory),
    directory_file_path(LabsDirectory, '..', PrologDirectory),
    directory_file_path(PrologDirectory, '..', V6Directory),
    directory_file_path(V6Directory, '..', RepositoryDirectory),
    directory_file_path(RepositoryDirectory, Name, Path).

contains_all(Text, Substrings) :-
    forall(member(Substring, Substrings), sub_string(Text, _, _, _, Substring)).

inventory_receipt :-
    repo_file('src/db.rs', DbPath),
    read_file_to_string(DbPath, DbText, []),
    findall(Name, expected_registration(Name), Names),
    length(Names, 14),
    contains_all(DbText, Names),
    lab_file('v5-capture.jsonl', CapturePath),
    jq_lines(CapturePath,
       '.[0].v5_registrations | length == 14 and (sort == ["regexp","sprf_lcfirst","sprf_lines","sprf_lower","sprf_norm","sprf_replace_re","sprf_split","sprf_strip_prefix","sprf_strip_suffix","sprf_sym","sprf_sym_intern","sprf_trim","sprf_ucfirst","sprf_upper"])').

capture_receipt :-
    lab_file('v5-capture.jsonl', CapturePath),
    jq_lines(CapturePath,
       'length == 226 and (map(select(.kind == "value")) | length == 224) and (.[0].bare_sqlite_version == "3.46.0") and ((.[0].bare_core_functions | map(.name) | index("regexp")) == null) and ((.[0].bare_core_functions | map(.name) | index("sprf_split")) == null) and (.[-1].result == "sidecar:ok")').

driver_receipt :-
    sqlite_udf_lab_directory(LabDirectory),
    lab_file('node/probe.mjs', ProbePath),
    lab_file('node-driver-probe.json', ProbeReceipt),
    run_node(ProbePath, ['--node-root', LabDirectory, '--out', ProbeReceipt]),
    jq(ProbeReceipt,
       '(.results | map(select(.candidate == "@libsql/client@0.17.4"))[0].methods.createFunction == "undefined") and (.results | map(select(.candidate == "@libsql/client@0.17.4"))[0].unknown_function_error != null) and (.results | map(select(.candidate == "better-sqlite3"))[0].registered == true) and (.results | map(select(.candidate == "sql.js"))[0].registered == true) and (.results | map(select(.candidate == "node-sqlite3"))[0].executed == false)').

graft_receipt :-
    repo_file('', RepositoryPath),
    lab_file('node/graft-check.mjs', GraftPath),
    lab_file('graft-check.json', GraftReceipt),
    run_node(GraftPath, ['--root', RepositoryPath, '--out', GraftReceipt]),
    jq(GraftReceipt,
       '(.corpus_rows == 16) and (.oracle_values == 224) and (.deltaChecks.sql_native == true) and (.deltaChecks.udf == true) and (.deltaChecks.ts_deopt.full_table_scan == false) and (.deltaChecks.emit_time.constant_arguments == true) and ([.sourceReceipts[]] | all)').

conformance_receipt :-
    repo_file('v6/prolog/conformance/go.pl', ConformancePath),
    run_swipl(ConformancePath).

run_node(ScriptPath, Arguments) :-
    sqlite_udf_lab_directory(LabDirectory),
    process_create(path(node), [ScriptPath | Arguments],
                   [cwd(LabDirectory), stdout(null), stderr(null), process(ProcessId)]),
    process_wait(ProcessId, exit(0)).

run_swipl(ScriptPath) :-
    process_create(path(swipl), ['-q', '-l', ScriptPath, '-g', 'go', '-g', 'halt'],
                   [stdout(null), stderr(null), process(ProcessId)]),
    process_wait(ProcessId, exit(0)).

jq(JsonPath, Filter) :-
    process_create(path(jq), ['-e', Filter, JsonPath],
                   [stdout(null), stderr(null), process(ProcessId)]),
    process_wait(ProcessId, exit(0)).

jq_lines(JsonPath, Filter) :-
    process_create(path(jq), ['-e', '-s', Filter, JsonPath],
                   [stdout(null), stderr(null), process(ProcessId)]),
    process_wait(ProcessId, exit(0)).

pass(Name, Check) :-
    ( call(Check)
    -> format('PASS ~w~n', [Name])
    ;  halt(1)
    ).

go :-
    pass(inventory, inventory_receipt),
    pass(capture, capture_receipt),
    pass(drivers, driver_receipt),
    pass(graft, graft_receipt),
    pass(conformance, conformance_receipt).
