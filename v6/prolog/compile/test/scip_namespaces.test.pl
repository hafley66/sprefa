% FAIL-FIRST: sh_head//2 read a bare ident, so a multi-segment executor path
% did not parse at all and the first test below threw dl_parse_error before
% reaching an assertion.
:- module(scip_namespaces_tests, []).

:- use_module(library(plunit)).
:- use_module('../../next/0_parse/parse_dl_dcg', [parse_dl_dcg_entry/5]).
:- use_module('../../next/0_parse/use_resolve', [expand_uses/8]).
:- use_module('../registry',
              [ host_input_contract/3,
                host_output_contract/3,
                scip_namespace_host/3
              ]).
:- use_module('../../next/1_expand/1_host_expand', [compile_host_decl/2]).

:- begin_tests(scip_namespaces).

scip_source(Source) :-
    Source = "rel /scip/diet/call(repo: text, path: text, digest: text)\c
\n  -> (record: text, family: text, caller_path: text, callee_path: text,\c
\n      callee: text, kind: text) key(1, 2).\c
\nrel edge(callee: text).\c
\nedge(Callee) <-\c
\n  /scip/diet/call('.', 'a.rs', 'd', 'resolved_edge', 'call',\c
\n                 _Caller, _Callee, Callee, _Kind).\n".

% The dl6 spelling is slash-rooted; the atom every phase below the parser
% carries is module_path_name/2's `__` join, a legal SQL and Rust identifier.
test(slash_executor_path_parses_to_the_module_path_atom) :-
    scip_source(Source),
    string_codes(Source, Codes),
    parse_dl_dcg_entry(scip_test, Codes, Prog, _Bindings, Findings),
    Findings == [],
    Prog = program(Decls, _Rules, _Queries),
    memberchk(arrival_identity(scip__diet__call, [1, 2]), Decls),
    memberchk(sh_decl(scip__diet__call, Inputs, Outputs, _Template), Decls),
    Inputs == [col(repo, text), col(path, text), col(digest, text)],
    Outputs == [col(record, text), col(family, text), col(caller_path, text),
                col(callee_path, text), col(callee, text), col(kind, text)].

% An executor path GOAL is a name with segments, never a nested rel: it
% flattens to the same atom before host normalization reads the goal's functor.
test(slash_executor_goal_becomes_a_probe_on_the_flat_name) :-
    scip_source(Source),
    string_codes(Source, Codes),
    parse_dl_dcg_entry(scip_test, Codes, program(_Decls, Rules, _Queries),
                       _Bindings, _Findings),
    memberchk(Rule, Rules),
    Rule =.. ['<-', _Head, Body],
    Body = probe(scip__diet__call, Ins, Outs, Salts),
    Ins == ['.', 'a.rs'],
    Salts == [salt(digest, d)],
    length(Outs, 6).

% Ruling executor_modules_use_import: one segment list, two path spellings,
% both still legal beside the `use` form.
test(a_slash_path_and_a_dotted_path_reach_one_atom) :-
    forall(member(Spelling-Expected,
                  ["/scip/diet/call"-scip__diet__call,
                   "scip.diet.call"-scip__diet__call,
                   "/soopy/files"-soopy__files]),
           ( string_codes(Spelling, PathCodes),
             once(phrase(parse_dl_dcg:dotted_path(Segments), PathCodes)),
             atomic_list_concat(Segments, '__', Expected) )).

test(every_scip_namespace_carries_the_repo_extract_contract) :-
    forall(scip_namespace_host(Name, _Interface, _Evidence),
           ( host_input_contract(Name, Columns, Roles),
             Columns == [col(repo, text), col(path, text), col(digest, text)],
             Roles == [identity, identity, freshness] )).

% THE INTERFACING TYPE: one `<x>`, one column list, whichever evidence answers.
% A program swaps /scip/diet/call for /scip/call by changing the host name alone.
test(both_namespaces_of_one_interface_declare_equal_columns) :-
    forall(member(Interface, [call, type]),
           ( scip_namespace_host(Diet, Interface, diet),
             scip_namespace_host(Index, Interface, index),
             Diet \== Index,
             host_output_contract(Diet, Interface, DietColumns),
             host_output_contract(Index, Interface, IndexColumns),
             DietColumns == IndexColumns,
             host_input_contract(Diet, DietInputs, DietRoles),
             host_input_contract(Index, IndexInputs, IndexRoles),
             DietInputs == IndexInputs,
             DietRoles == IndexRoles )).

test(the_call_interface_is_the_receiver_shape) :-
    once(host_output_contract(scip__call, call, Columns)),
    Columns == [col(record, text), col(family, text), col(caller_path, text),
                col(callee_path, text), col(callee, text), col(kind, text)].

% The emitted demand and response rel names are SQL identifiers, so no path
% separator may reach them; `__` is the join the rest of the compiler uses.
test(executor_path_emits_sql_safe_relation_names) :-
    compile_host_decl(
      sh_decl(scip__diet__call,
              [col(repo, text), col(path, text), col(digest, text)],
              [col(record, text), col(family, text), col(caller_path, text),
               col(callee_path, text), col(callee, text), col(kind, text)],
              template("cd {repo} && extract {path}")),
      Plan),
    Plan = host_plan(_Name, _Inputs, _Outputs, _Template,
                     demand_ref(DemandRel), response_ref(ResponseRel),
                     _Roles),
    DemandRel == '__host_demand_scip__diet__call',
    ResponseRel == '__host_response_scip__diet__call',
    forall(member(Rel, [DemandRel, ResponseRel]),
           ( atom_codes(Rel, Codes),
             forall(member(Code, Codes),
                    ( code_type(Code, alnum) ; Code =:= 0'_ )) )).

% The rail file and the registry are one list, so neither can drift alone.
test(receiver_rail_declares_the_registry_columns) :-
    module_property(scip_namespaces_tests, file(TestFile)),
    file_directory_name(TestFile, TestDir),
    atomic_list_concat([TestDir, '/../../../dl/deadcode/receiver-rail.dl6'], Rail),
    % Through the whole door: `use scip.` binds the leaf, and the raw parser
    % alone reports the file's local name instead of the registry's.
    expand_uses(Rail, [], [], _, program(Decls, _, _), _, _, _),
    forall(member(Host, [scip__call, scip__diet__call]),
           ( memberchk(sh_decl(Host, Inputs, Outputs, _), Decls),
             once(host_input_contract(Host, Inputs, _Roles)),
             once(host_output_contract(Host, call, Outputs)) )).

:- end_tests(scip_namespaces).
