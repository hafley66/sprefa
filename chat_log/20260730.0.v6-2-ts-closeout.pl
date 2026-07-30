% V6.2 TypeScript closeout ledger.
% Load:
%   swipl -q -l chat_log/20260730.0.v6-2-ts-closeout.pl
% Queries:
%   locked(Name, Contract).
%   lane(Name, State, Deliverable).
%   task(Name, State, Exit).
%   verification(Name, Result).

:- module(session_20260730_0, []).

session(id, '20260730.0').
session(date, '2026-07-30').
session(branch, 'codex/rel-ref-file-span-lab').
session(topic, v6_2_ts_closeout).
session(next_version, '7-rust').

goal(v6_2,
     'finish the TypeScript compiler/runtime prototype, extraction parity, golden programs, and target-neutral boundary required by the Rust backend').

locked(host_relation_surface,
       'a registered host relation is written as an ordinary RHS relation atom; its contract selects demand-response lowering').
locked(no_rhs_probe_marker,
       'remove RHS question-mark probe syntax; top-level question-mark remains the query command').
locked(no_salt_rider,
       'remove at-sign salt syntax; host contract identity columns are ordinary typed inputs').
locked(host_contract,
       'signature carries input/output columns, types, identity projection, clock transition, executor key, demand relation, and response relation').
locked(target_order,
       'V6.2 executes target-neutral plans in TypeScript; V7 consumes the identical checked plan in Rust').
locked(extraction_boundary,
       'TypeScript may spawn the released sprefa-extract binary; Rust later links sprefa-extract as a library without changing facts or clocks').
locked(parser_residency,
       'all language analysis and parsing lives in Rust without exception, including Markdown, XML, HTML, configuration formats, and programming languages; TypeScript schedules demands and consumes typed fact rows only').
locked(sprefa_extract_scope,
       'sprefa-extract remains the complete high-performance Rust extraction and language-analysis boundary; V6.2 invokes it as a process and V7 links the same library').
locked(dsl_rule,
       'new DSL surface expands in Prolog into existing relations before analysis; emitters consume checked target-neutral IR').
locked(surface_freeze,
       'agents may remove redundant syntax or reuse existing syntax; no agent may introduce new syntax').
locked(new_syntax_stop_rule,
       'if implementation appears to require new syntax, stop production edits, lab the fork against the existing world, write a plan with costs and receipts, and return for user ruling').
locked(arch_landing_rule,
       'every completed lane updates its task state and evidence in v6/prolog/ARCH.pl through the coordinator').
locked(checkpoint_rule,
       'commit completed isolated landings frequently; exclude unrelated dirty files and generated output unless the landing owns them').

question(scan_relation,
         'whether scan becomes an ordinary registered relation or compile-time relation expansion; keep open until exact-one state, init, and clock gaps are discharged').
question(match_relation_expression,
         'whether match can be exposed as a relation-valued expression; current left-to-right match remains expansion into ordinary rules').

lane(host_surface_cleanup, active,
     'plain RHS host atom becomes internal probe IR; remove canonical RHS question-mark; migrate fixtures and roundtrip tests').
lane(host_contract_executor, active,
     'target-neutral executor-key host plans plus built-in extraction contracts and TypeScript executor registry').
lane(v6_2_exit_audit, active,
     'ordered required implementation, parity gates, bugs, and V7 deferrals from ARCH, scoreboard, and golden plans').

task(remove_rhs_question_mark, active,
     'zero executable RHS host probes use question-mark syntax; compiler infers host lowering from the registered contract').
task(remove_at_salt, queued,
     'zero executable at-sign salt riders; every former salt is a contract identity input with unchanged cache receipts').
task(builtin_extraction_relations, active,
     'call, type, df, cst, and resolved-edge host contracts require no per-program shell declaration').
task(target_neutral_host_plan, active,
     'checked plan contains executor keys and contracts without TypeScript or Rust execution code').
task(extraction_parity, active,
     'V5 questions port through built-in extraction facts with exact classified parity receipts').
task(clock_and_type_gate, active,
     'host demand/response clocks, bool/float types, and relative-hop proofs have executable gates').
task(golden_gate, active,
     'ghcacher and extraction golden programs run through the served TypeScript runtime').
task(grafana_scale_gate, active,
     'pinned Grafana crawl passes with wall time, peak RSS, SQLite bytes, throughput, tick or write amplification, and exact final-row receipts').
task(file_watcher_scale_gate, active,
     'file watcher passes cardinality and edit-churn sweeps with bounded subscriptions, events, extraction demands, RSS, database growth, and zero duplicate or stale facts').
task(http_cli_dogfood, active,
     'bop exercises program load, arrivals, relation query, tick stream, and stats through HTTP; command and route inventories are generated from canonical Prolog facts while handlers remain explicit').
task(rust_backend, deferred_v7,
     'emit Rust from the identical checked plan and link sprefa-extract directly').

observed(at_surface,
         '14 executable salt riders exist in V6 DL fixtures; remaining at-sign text is historical prose or ast-grep capture syntax').
observed(probe_surface,
         '35 RHS question-mark occurrences exist across 10 V6 DL fixtures before migration').
observed(extraction_clock_golden,
         'three ticks prove two extracted facts followed by retraction to empty after the file edit').
observed(ghcacher_clock_golden,
         'five scheduled ticks produce the pinned final cache state').
observed(v5_cache_scale_harness,
         'bench/0_gh_cache_scale.sh synthesizes deterministic endpoints and records engine time, process wall, RSS, SQLite bytes, calls, misses, and write rows per tick').
observed(http_cli_current,
         'v6/tsv2/cli/bop.ts has serve, run, check, load, and q; run/load/q call the served HTTP interface, while stats and ticks have no CLI verb').
observed(cli_inventory_current,
         'registry.pl cli_command/3 and Commander declarations are hand-mirrored and checked by a source-text inventory test; generation has not replaced the mirror').

verification(v5_cache_scale_smoke,
             passed(endpoints(5), calls(10), misses(0), db_mib(1.4), rss_mib(30.7))).
verification(v5_cache_stress_regression, passed(1)).
verification(rel_definition_hash_lab, passed(11)).
verification(generic_scan_instantiation_lab, passed(11)).
verification(select_scan_cache_lab, passed(executable(12), plan(13))).

next_action(1, 'merge and grade the no-question-mark host surface lane').
next_action(2, 'make identity columns replace every at-sign salt rider and rerun extraction/host clock goldens').
next_action(3, 'merge target-neutral executor contracts and built-in extraction relation signatures').
next_action(4, 'execute the V6.2 exit audit in dependency order').
next_action(5, 'run pinned Grafana and file-watcher scale gates and record machine-readable baselines').
next_action(6, 'commit checkpoints, update ARCH task states, and push the branch').
