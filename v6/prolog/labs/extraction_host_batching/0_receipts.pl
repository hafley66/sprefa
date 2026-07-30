% 0_receipts.pl : extraction host process-amplification lab.
%
% Run:
%   swipl -q -l v6/prolog/labs/extraction_host_batching/0_receipts.pl -g go -g halt
%
% The lab reads production sources and executes the in-tree release extractor.
% It does not alter the compiler, runtime, extractor, or DL surface.

:- module(extraction_host_batching_lab, [go/0]).

:- use_module('../../compile/registry', [host_execution/3]).
:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module(library(readutil), [read_file_to_string/3]).

go :-
    fixture_census_receipt,
    current_cardinality_receipt,
    executor_boundary_receipt,
    multi_family_equivalence_receipt,
    heterogeneous_projection_receipt,
    invocation_group_receipt,
    freshness_separation_receipt,
    relational_response_receipt,
    rtkq_absence_receipt,
    format("9 PASS~n").

repo_root(Root) :-
    source_file(extraction_host_batching_lab:go, Source),
    file_directory_name(Source, LabDir),
    absolute_file_name('../../../..', Root,
                       [relative_to(LabDir), file_type(directory)]).

repo_file(Relative, Absolute) :-
    repo_root(Root),
    directory_file_path(Root, Relative, Absolute).

fixture_text(Relative, Text) :-
    repo_file(Relative, Absolute),
    read_file_to_string(Absolute, Text, []).

count_substring(Text, Needle, Count) :-
    findall(Start, sub_string(Text, Start, _, _, Needle), Starts),
    length(Starts, Count).

sh_declaration_count(Text, Count) :-
    split_string(Text, "\n", "\r", Lines),
    include(starts_with_sh, Lines, Declarations),
    length(Declarations, Count).

starts_with_sh(Line) :-
    sub_string(Line, 0, 3, _, "sh ").

fixture_census_receipt :-
    fixture_text('v6/dl/fixtures/flagship-callgraph.dl6', Callgraph),
    sh_declaration_count(Callgraph, 3),
    count_substring(Callgraph,
                    "`\"$DL_EXTRACT_BIN\" --family cst,type,call,df {path}`", 2),
    fixture_text('v6/dl/fixtures/diag-rail.dl6', Diag),
    sh_declaration_count(Diag, 2),
    count_substring(Diag,
                    "`\"$DL_EXTRACT_BIN\" --family cst,type,call,df {path}`", 2),
    fixture_text('v6/dl/fixtures/flagship-flow.dl6', Flow),
    sh_declaration_count(Flow, 9),
    count_substring(Flow,
                    "`\"$DL_EXTRACT_BIN\" --family cst,type,call,df {path}`", 7),
    count_substring(Flow, "--resolve", 1),
    fixture_text('v6/dl/fixtures/0_extraction-clock-golden.dl6', Golden),
    sh_declaration_count(Golden, 1),
    count_substring(Golden,
                    "`\"$DL_EXTRACT_BIN\" --family call {path}`", 1),
    format("PASS exact fixture host declaration and extractor command census~n").

% host/extract cardinality at N distinct path+digest inputs and S seed rows.
% The resolve command is one HostRunner execution per seed. Its xargs child can
% split at ARG_MAX, so extractor_processes below counts the one-file fixture
% case where xargs emits one child.
current_counts(flagship_callgraph, N, 1, HostRuns, ExtractRuns) :-
    HostRuns is 1 + N,
    ExtractRuns is N.
current_counts(diag_rail, N, 1, HostRuns, ExtractRuns) :-
    HostRuns is N,
    ExtractRuns is N.
current_counts(flagship_flow, N, 1, HostRuns, ExtractRuns) :-
    HostRuns is 2 + N,
    ExtractRuns is 1 + N.
current_counts(extraction_clock_golden, _N, Versions, HostRuns, ExtractRuns) :-
    HostRuns is Versions,
    ExtractRuns is Versions.

fanout_counts(flagship_callgraph, N, 1, HostRuns, ExtractRuns) :-
    HostRuns is 1 + N,
    ExtractRuns is N.
fanout_counts(diag_rail, N, 1, HostRuns, ExtractRuns) :-
    HostRuns is N,
    ExtractRuns is N.
fanout_counts(flagship_flow, N, 1, HostRuns, ExtractRuns) :-
    HostRuns is 2 + N,
    ExtractRuns is 1 + N.
fanout_counts(extraction_clock_golden, _N, Versions, HostRuns, ExtractRuns) :-
    HostRuns is Versions,
    ExtractRuns is Versions.

current_cardinality_receipt :-
    current_counts(flagship_callgraph, 1, 1, 2, 1),
    current_counts(diag_rail, 1, 1, 1, 1),
    current_counts(flagship_flow, 1, 1, 3, 2),
    current_counts(extraction_clock_golden, 1, 2, 2, 2),
    fanout_counts(flagship_callgraph, 1, 1, 2, 1),
    fanout_counts(diag_rail, 1, 1, 1, 1),
    fanout_counts(flagship_flow, 1, 1, 3, 2),
    fanout_counts(extraction_clock_golden, 1, 2, 2, 2),
    format("PASS landed one-process-per-path formulas are pinned~n").

executor_boundary_receipt :-
    Template = "\"$DL_EXTRACT_BIN\" --family cst,type,call,df {path}",
    host_execution(extract, Template, sprefa_extract),
    host_execution(call_node, Template, sprefa_extract),
    host_execution(call_ref, Template, sprefa_extract),
    host_execution(df_node_at, Template, sprefa_extract),
    host_execution(local_shell, "printf ok {path}", shell),
    format("PASS extractor templates share one executor while generic shell remains separate~n").

extract_lines(Families, Lines) :-
    repo_file('v6/sprefa-extract/target/release/extract', Extract),
    access_file(Extract, execute),
    repo_file('v6/sprefa-extract/tests/fixtures/ts/sample.ts', Sample),
    process_create(
        Extract,
        ['--family', Families, Sample],
        [stdout(pipe(Out)), stderr(null), process(Pid)]),
    read_string(Out, _, Text),
    close(Out),
    process_wait(Pid, exit(0)),
    split_string(Text, "\n", "\r\t ", RawLines),
    exclude(=(""), RawLines, Lines).

multi_family_equivalence_receipt :-
    extract_lines("df,call,type", Combined),
    extract_lines("df", Df),
    extract_lines("call", Call),
    extract_lines("type", Type),
    append([Df, Call, Type], Separate),
    msort(Combined, Sorted),
    msort(Separate, Sorted),
    Combined \= [],
    format("PASS one existing multi-family extractor run equals three family runs~n").

json_dict(Line, Dict) :-
    atom_string(Atom, Line),
    atom_json_dict(Atom, Dict, []).

carries(Dict, Columns) :-
    forall(member(Column, Columns),
           (get_dict(Column, Dict, Value), Value \== null)).

projection_rows(Lines, Columns, Rows) :-
    findall(
        Row,
        ( member(Line, Lines),
          json_dict(Line, Dict),
          carries(Dict, Columns),
          maplist(dict_value(Dict), Columns, Row)
        ),
        Rows).

dict_value(Dict, Column, Value) :-
    get_dict(Column, Dict, Value).

heterogeneous_projection_receipt :-
    extract_lines("call", Lines),
    projection_rows(Lines, [record, family, kind, name], Nodes),
    projection_rows(Lines, [record, family, callee], Sites),
    Nodes \= [],
    Sites \= [],
    forall(member([Record, Family, _, _], Nodes),
           (Record == "node", Family == "call")),
    forall(member([Record, Family, _], Sites),
           (Record == "site", Family == "call")),
    format("PASS one heterogeneous stdout projects into two typed row shapes~n").

% A process group excludes the host name and output projection. It includes
% executor, template, and every ordered input value, including freshness.
invocation_key(
    plan(_Name, Executor, Template, Inputs, _Outputs, _ResponseRel),
    invocation(Executor, Template, Inputs)).

unique_invocations(Plans, Count) :-
    maplist(invocation_key, Plans, Keys),
    sort(Keys, Unique),
    length(Unique, Count).

invocation_group_receipt :-
    Inputs = [path-"sample.ts", digest-"abc"],
    Plans = [
        plan(call_node, sprefa_extract, all_families, Inputs,
             [record, family, kind, name], response_call_node),
        plan(call_ref, sprefa_extract, all_families, Inputs,
             [record, family, callee], response_call_ref),
        plan(df_param, sprefa_extract, all_families, Inputs,
             [record, family, pos], response_df_param),
        plan(type_sig, sprefa_extract, all_families, Inputs,
             [record, family, owner_start, owner_end, slot, pos, ty],
             response_type_sig)
    ],
    unique_invocations(Plans, 1),
    format("PASS output projections fan out behind one internal invocation key~n").

freshness_separation_receipt :-
    Plans = [
        plan(call_node, sprefa_extract, all_families,
             [path-"sample.ts", digest-"old"], [record], response_a),
        plan(call_ref, sprefa_extract, all_families,
             [path-"sample.ts", digest-"new"], [callee], response_b),
        plan(call_ref, sprefa_extract, all_families,
             [path-"other.ts", digest-"new"], [callee], response_c)
    ],
    unique_invocations(Plans, 3),
    format("PASS digest and path changes cannot share a process result~n").

relational_response_receipt :-
    extract_lines("call", Lines),
    projection_rows(Lines, [record, family, kind, name], NodeRows),
    projection_rows(Lines, [record, family, callee], SiteRows),
    maplist(wrap_response(call_node_response), NodeRows, NodeResponses),
    maplist(wrap_response(call_ref_response), SiteRows, SiteResponses),
    forall(member(Response, NodeResponses), Response =.. [call_node_response | _]),
    forall(member(Response, SiteResponses), Response =.. [call_ref_response | _]),
    \+ ( member(Response, NodeResponses), functor(Response, json, _) ),
    \+ ( member(Response, SiteResponses), functor(Response, json, _) ),
    format("PASS fanout lands ordinary relation rows; JSON remains transient wire input~n").

wrap_response(Rel, Values, Row) :-
    Row =.. [Rel | Values].

rtkq_absence_receipt :-
    repo_file('v6/dl/fixtures/1_rtkq-extraction-golden.dl6', Expected),
    exists_file(Expected),
    fixture_text('v6/dl/fixtures/1_rtkq-extraction-golden.dl6', Text),
    sh_declaration_count(Text, 1),
    count_substring(Text, "--ast-pattern", 4),
    format("PASS V6 RTKQ golden uses one extractor host with four ast patterns~n").
