% Extraction host batching lab record.
%
% Validate:
%   swipl -q -l plans/2026-07-30-extraction-host-batching-lab.pl -g go -g halt
%
% Execute receipts:
%   swipl -q -l v6/prolog/labs/extraction_host_batching/0_receipts.pl -g go -g halt

:- module(extraction_host_batching_lab_plan,
          [ go/0,
            goal/2,
            measured/5,
            finding/3,
            implementation_step/3,
            invariant/2
          ]).

:- use_module(library(aggregate), [aggregate_all/3]).

goal(cardinality,
     'pin host demands and extractor process launches for each V6.2 extraction rail').
goal(fanout,
     'run one Rust extraction per path and digest, then project its tagged JSONL into ordinary typed response relations').
goal(boundary,
     'reuse the current rel checker and SQL lowering; add no surface syntax, stored JSON, special relation value, or second type system').

% measured(Fixture, Unit, CurrentHostRuns, CurrentExtractorRuns, FanoutRuns).
% N is the count of distinct path+digest rows. V is the count of file versions.
measured(flagship_callgraph, n_files,
         '1 + 2*N', '2*N', 'host 1+N; extract N').
measured(diag_rail, n_files,
         '2*N', '2*N', 'host N; extract N').
measured(flagship_flow, n_files_one_seed,
         '2 + 7*N', '1 + 7*N', 'host 2+N; extract 1+N').
measured(extraction_clock_golden, v_versions,
         'V', 'V', 'host V; extract V').
measured(rtkq_v6, absent,
         '0', '0', '0 until a V6 golden exists').

finding(extractor_cli, present,
        '`--family` already accepts comma-separated cst,type,call,df and one dispatch/flatten emits the union').
finding(current_runtime, n_plus_one,
        'HostRunner keys work by host name plus witness, then concatMaps one subprocess per demand').
finding(current_plan, sufficient_projection_metadata,
        'each host plan already carries ordered inputs, output columns, response relation, template, and executor key').
finding(current_executor_registry, gap,
        'only the host name extract selects sprefa_extract; call_node, call_ref, and flow projections currently select generic shell').
finding(rtkq, absent,
        'no V6 RTKQ DL6 fixture or extraction golden is present, so it has no current host demand to measure').

implementation_step(0, compiler_internal,
                    'classify the fixed DL_EXTRACT_BIN template as executor sprefa_extract after validating its path,digest input contract').
implementation_step(1, fixtures,
                    'give every projection for one path,digest the same existing multi-family command template; no DL grammar change').
implementation_step(2, runtime,
                    'within one boot batch or engine tick, group sprefa_extract demands by executor, template, and every ordered input value').
implementation_step(3, runtime,
                    'spawn once per group, decode the same stdout against each plan output projection, and submit all response rel arrivals in one engine batch').
implementation_step(4, durability,
                    'claim and settle each existing host-name+witness cache row separately; cached projections may be omitted from a group without changing the remaining rows').

invariant(one_type_system,
          'projected outputs enter declared response rels and follow the current rel-to-SQL lowering').
invariant(no_json_storage,
          'JSONL exists only between Rust stdout and response projection; no JSON document becomes program state').
invariant(freshness,
          'the process key includes digest even when the shell template reads only path').
invariant(no_cross_path_batch,
          'ordinary extraction remains one Rust call per path,digest; project resolve remains its separate corpus call').
invariant(shell_safety,
          'generic shell executions never coalesce because repeated commands may carry effects').
invariant(no_semantic_card,
          'the lab leaves zero semantic questions: the remaining work changes compiler/runtime scheduling behind existing host contracts').

go :-
    aggregate_all(count, goal(_, _), 3),
    aggregate_all(count, measured(_, _, _, _, _), 5),
    aggregate_all(count, finding(_, _, _), 5),
    aggregate_all(count, implementation_step(_, _, _), 5),
    aggregate_all(count, invariant(_, _), 6),
    format("PASS extraction host batching plan integrity~n").
