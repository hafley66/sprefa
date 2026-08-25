% Compiler contract goldens for structured host request/response values.
:- module(typed_host_contracts_tests, []).

:- use_module(library(plunit)).
:- use_module('../../2_host_expand/1_host_expand',
              [ compile_host_decl/2, host_plan_contract/2 ]).
:- use_module('../../emit_ts', []).
:- use_module('../../0_dot_expand/registry', [host_execution/3]).

:- begin_tests(typed_host_contracts).

test(nested_struct_request_keeps_field_layout) :-
    compile_host_decl(
      sh_decl(stage,
              [col(root, text), col(request, stage_request)],
              [col(result, stage_result)],
              template("stage {root} {request}")),
      Plan),
    host_plan_contract(
      Plan,
      host_contract(
        type_descriptor('__host_demand_stage'/2,
                        [field(root, text), field(request, stage_request)]),
        type_descriptor('__host_response_stage'/1,
                        [field(result, stage_result)]))).

test(discriminated_union_and_bytes_are_declared_types) :-
    compile_host_decl(
      sh_decl(commit,
              [col(request, commit_request), col(payload, bytes)],
              [col(outcome, commit_result), col(receipt, bytes)],
              template("commit {request} {payload}")),
      Plan),
    host_plan_contract(
      Plan,
      host_contract(
        type_descriptor('__host_demand_commit'/2,
                        [field(request, commit_request), field(payload, bytes)]),
        type_descriptor('__host_response_commit'/2,
                        [field(outcome, commit_result), field(receipt, bytes)]))).

test(nested_list_option_and_enum_spelling_survives_ir) :-
    Plan = host_plan(batch,
                     [col(payload, list(option(bytes))), col(choice, result)],
                     [col(value, option(list(result)))],
                     template("batch {payload} {choice}"),
                     demand_ref('__host_demand_batch'),
                     response_ref('__host_response_batch'),
                     input_roles([identity, identity])),
    host_plan_contract(
      Plan,
      host_contract(
        type_descriptor('__host_demand_batch'/2,
                        [field(payload, list(option(bytes))), field(choice, result)]),
        type_descriptor('__host_response_batch'/1,
                        [field(value, option(list(result)))]))).

test(structured_program_json_carries_descriptors) :-
    compile_host_decl(
      sh_decl(stage,
              [col(root, text), col(request, stage_request)],
              [col(result, stage_result)],
              template("stage {root} {request}")),
      Plan),
    Plan = host_plan(stage, Inputs, Outputs, template(Template),
                     demand_ref(DemandName), response_ref(ResponseName), _),
    emit_ts:host_columns_json(Inputs, _InputsJson),
    emit_ts:host_columns_json(Outputs, _OutputsJson),
    emit_ts:js_string(Template, _TemplateJson),
    emit_ts:js_string(DemandName, _DemandJson),
    emit_ts:js_string(ResponseName, _ResponseNameJson),
    host_execution(stage, Template, _Executor),
    host_plan_contract(Plan, host_contract(RequestType, ResponseType)),
    emit_ts:host_contract_is_structured(RequestType, ResponseType),
    emit_ts:host_type_descriptor_json(RequestType, _RequestJson),
    emit_ts:host_type_descriptor_json(ResponseType, _ResponseDescriptorJson),
    emit_ts:host_plan_json(Plan, Json),
    once(( sub_atom(Json, _, _, _, 'request_type'),
           sub_atom(Json, _, _, _, '"__host_demand_stage/2"'),
           sub_atom(Json, _, _, _, 'response_type') )).

test(structured_program_json_preserves_catalog_refs_and_nested_layout) :-
    Plan = host_plan(batch,
                     [col(payload, list(option(bytes))), col(choice, result)],
                     [col(value, option(list(result)))],
                     template("batch {payload} {choice}"),
                     demand_ref('__host_demand_batch'),
                     response_ref('__host_response_batch'),
                     input_roles([identity, identity])),
    emit_ts:host_plan_json(Plan, Json),
    Json = '{ name: "batch", inputs: [{ name: "payload", type: "list(option(bytes))" }, { name: "choice", type: "result" }], outputs: [{ name: "value", type: "option(list(result))" }], template: "batch {payload} {choice}", demand_rel: "__host_demand_batch", response_rel: "__host_response_batch", execution: "shell", request_type: { ref: "__host_demand_batch/2", fields: [{ name: "payload", type: "list(option(bytes))" }, { name: "choice", type: "result" }] }, response_type: { ref: "__host_response_batch/1", fields: [{ name: "value", type: "option(list(result))" }] } }'.

test(scalar_program_json_is_byte_compatible) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(path, text), col(bucket, int)],
              [col(status, text)],
              template("fetch {path} {bucket}")),
      Plan),
    emit_ts:host_plan_json(Plan, Json),
    \+ sub_atom(Json, _, _, _, 'request_type'),
    Json = '{ name: "fetch", inputs: [{ name: "path", type: "text" }, { name: "bucket", type: "int" }], outputs: [{ name: "status", type: "text" }], template: "fetch {path} {bucket}", demand_rel: "__host_demand_fetch", response_rel: "__host_response_fetch", execution: "shell" }'.

test(scalar_shell_host_keeps_legacy_layout) :-
    compile_host_decl(
      sh_decl(fetch,
              [col(path, text), col(bucket, int)],
              [col(status, text)],
              template("fetch {path} {bucket}")),
      Plan),
    host_plan_contract(
      Plan,
      host_contract(
        type_descriptor('__host_demand_fetch'/2,
                        [field(path, text), field(bucket, int)]),
        type_descriptor('__host_response_fetch'/1,
                        [field(status, text)]))).

test(invalid_executor_shape_is_rejected_before_invocation,
     [throws(host_executor_mismatch(extract, sprefa_extract, [col(file, text)]))]) :-
    compile_host_decl(
      sh_decl(extract,
              [col(file, text)],
              [col(result, text)],
              template("extract {file}")),
      _).

:- end_tests(typed_host_contracts).
