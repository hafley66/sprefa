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
locked(parallel_rule,
       'run independent compiler, runtime, extraction, parity, benchmark, and audit lanes concurrently when their file ownership does not overlap').
locked(host_shell_accounting,
       'every landing reports new registered hosts, local shell declarations, executor bindings, and subprocesses; overreliance on host or shell is recorded as a named note').
locked(extractor_trait_boundary,
       'sprefa-extract may grow extraction capabilities freely, but each addition preserves typed Rust inputs, outputs, family contracts, and trait-level seams without weakening its type system').
locked(json_interop_scope,
       'official JSON relations and types are a separate additive opt-out module; preparation is lab and plan only until user rules on semantics and any surface').
locked(rtkq_extraction_target,
       'RTKQ is an extraction golden and requires multiple advanced ast-grep forms; CST or query result unions may be expressed as ordinary Prolog relations').
locked(v5_string_match_scope,
       'V5 string-based match parity is excluded; the spammed string matching surface is deprecated and does not block V6.2').
locked(norm_parity,
       'V5 norm(text) is required runtime parity: lowercase ASCII alphanumerics and drop every other character; it uses existing expression-call surface and adds no syntax').
locked(single_rel_type_system,
       'there is one rel model and one checker; a rel column naming another rel denotes a generated relational edge whose nested source value is normalized into target rows and integer keys, with no relation-like intermediate type, JSON dictionary, or second type system').

question(scan_relation,
         'whether scan becomes an ordinary registered relation or compile-time relation expansion; keep open until exact-one state, init, and clock gaps are discharged').
question(match_relation_expression,
         'whether match can be exposed as a relation-valued expression; current left-to-right match remains expansion into ordinary rules').
question(nested_match_composition,
         'whether nested match needs any representation beyond generated named intermediate relations and ordinary rule expansion').
question(scan_match_composition,
         'whether scan reducers can consume match-derived 0/1/N rows while enforcing exactly one next state without higher-order runtime relation values').

lane(host_surface_cleanup, active,
     'plain RHS host atom becomes internal probe IR; remove canonical RHS question-mark; migrate fixtures and roundtrip tests').
lane(host_contract_executor, active,
     'target-neutral executor-key host plans plus built-in extraction contracts and TypeScript executor registry').
lane(v6_2_exit_audit, active,
     'ordered required implementation, parity gates, bugs, and V7 deferrals from ARCH, scoreboard, and golden plans').
lane(host_shell_extractor_census, active,
     'read-only accounting of registered hosts, sh declarations, subprocess amplification, sprefa-extract additions, and trait/type boundaries').
lane(json_interop_lab, active,
     'actual-world executable lab for an additive opt-out JSON module; no production or surface edits').
lane(coordinator_integration, active,
     'merge gates, run Grafana and watcher baselines, port norm, update ARCH and this ledger, commit checkpoints, and push').
lane(reference_runtime_finish, active,
     'finish or block the single-rel target-arrival then parent-edge normalization path; no intermediate relation type or new syntax').
lane(rtkq_extraction_golden, active,
     'grade multiple advanced ast-grep forms, relational union, spans, edits, retractions, and process counts through Rust extraction').
lane(host_batching_audit, active,
     'measure and reduce per-witness extraction subprocess amplification through existing typed executor contracts').

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
task(host_shell_census, active,
     'report every new host contract, shell declaration, executor, and sprefa-extract capability used to close V6.2, including avoidable shell process boundaries').
task(json_interop_lab, queued,
     'lab additive opt-out JSON relation/type interoperability against current rel, enum, reference, storage, host, clock, and SQLite semantics; make no production or surface edits').
task(rtkq_extraction_golden, active,
     'prove multiple advanced ast-grep query forms through typed sprefa-extract facts, relational unions, exact spans, edits, retractions, and bounded host/process amplification').
task(norm_runtime_parity, queued,
     'port V5 norm(text) through existing expression registration, SQLite lowering, TypeScript runtime, type checking, and exact V5 fixtures').
task(lab_reconciliation, active,
     'classify every open lab as implemented, canonical-plan, closed, or superseded; leave no unattached experiment directories or unindexed decisions').
task(scan_match_composition_reconcile, queued,
     'grade scan, match nesting, named intermediate relations, exact-one reducer output, initialization, clocks, and SQL lowering against current implementation before any syntax ruling').
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
verification(v6_2_exit_audit,
             baseline(conformance(164),
                      compiled(103),
                      identical(101),
                      wrong(0),
                      run_errors(2),
                      unsupported(61))).
verification(http_cli_dogfood, passed(8)).
verification(file_watcher_scale_test, passed(1)).
verification(file_watcher_scale_100,
             passed(events(480),
                    subscriptions(1),
                    ticks(3),
                    write_amplification(1.185185),
                    wall_ms(39.198),
                    rss_bytes(153845760),
                    sqlite_bytes(40960),
                    final_rows(90),
                    wrong_rows(0))).
verification(file_watcher_scale_1000,
             passed(events(4800),
                    subscriptions(1),
                    ticks(3),
                    write_amplification(1.185185),
                    wall_ms(265.241292),
                    rss_bytes(189202432),
                    sqlite_bytes(253952),
                    final_rows(900),
                    wrong_rows(0))).
verification(remote_checkpoint, pushed(c759bfcb)).
verification(host_shell_census,
             counted(executor_keys(2),
                     sh_declarations(33),
                     extract_declarations(14),
                     extract_fixture_files(5))).
verification(norm_runtime_parity,
             passed(plunit(167),
                    runtime(1),
                    host_changes(0),
                    parser_changes(0),
                    syntax_changes(0))).
verification(json_interop_lab,
             passed(plan_cards(5),
                    receipts(12),
                    decisions(0),
                    repeated_json_bytes(921600),
                    relational_ref_bytes(98304),
                    ratio(9.38))).
verification(grafana_scale_gate,
             passed(v5(files(42739), repos(389), wall_seconds(13.00),
                       files_per_second(3287.62), rss_bytes(356794368),
                       sqlite_bytes(52387840)),
                    v6(files(779), repos(8), wall_seconds(19.35),
                       files_per_second(40.26), rss_bytes(176553984),
                       sqlite_bytes(1069056), statements_per_tick(54.03)),
                    unusable_repos(139),
                    cap_excluded_repos(242))).
verification(host_contract_cleanup,
             passed(plunit(167),
                    conformance(all),
                    extraction_ticks(3),
                    ghcacher_ticks(5),
                    served_host(2))).
verification(ordered_pre,
             passed(runtime(2),
                    formerly_refused_fixtures(13),
                    syntax_changes(0),
                    type_changes(0))).
verification(scan_match_reconciliation,
             passed(executable(10),
                    plan_records(8),
                    open_cards(4),
                    selected_decisions(0),
                    nested_match_rel_tables(4),
                    nested_match_temp_tables(15),
                    scan_rel_tables(2),
                    scan_temp_tables(7))).
verification(reconciled_lab_receipts,
             passed(scan_match_value(8),
                    generic_scan(11),
                    rel_definition_hash(11),
                    select_scan_cache(12),
                    selected_decisions(0))).
verification(match_left_to_right_surface,
             passed(plunit(167),
                    g2_files(2),
                    g2_findings(0),
                    g1_pass(147),
                    g1_total(164),
                    known_removed_type_failures(17))).
verification(extraction_host_batching_lab,
             passed(receipts(9),
                    callgraph(current(2), batched(1)),
                    diagnostics(current(2), batched(1)),
                    flow(current_per_path(7), batched_per_path(1)),
                    syntax_changes(0),
                    type_models_added(0))).
host_overuse(grafana_v6_crawl,
             'green correctness gate exposes 40.26 files per second under the per-witness extraction subprocess boundary; batching or the V7 direct Rust link remains required').

host_executor(shell).
host_executor(sprefa_extract).
host_overuse(flagship_callgraph,
             'two separate --family call subprocesses per path and digest').
host_overuse(diag_rail,
             'two separate --family call subprocesses per path and digest').
host_overuse(flagship_flow,
             'seven per-path extraction subprocess declarations plus one project resolve declaration').
extractor_gap(document_sources,
              'no concrete Markdown, HTML, XML, TOML, YAML, or configuration Source implementation').
extractor_gap(file_blob_repo_revision,
              'BlobSource has no implementation; FileSet and ManifestMap are empty; Repo and Rev types are absent').

exit_order(1, host_surface_and_contract).
exit_order(2, ordered_pre_lowering).
exit_order(3, relation_reference_boundary).
exit_order(4, file_revision_blob_span_spine).
exit_order(5, builtin_extraction_relations).
exit_order(6, type_plane_and_ingest).
exit_order(7, clock_proof_facts).
exit_order(8, parity_and_scale_gates).
exit_order(9, stability_and_bug_gate).

exit_bug(fork_join_malformed_json).
exit_bug(log_retraction_run_error_classification).
exit_bug(reactor_buffertime_wall_clock_flake).
exit_bug(native_process_store_leak_flake).
exit_bug(v5_lsp_orphan).
exit_bug(swipl_gc_abort).
exit_bug(c7_durable_carry).

next_action(1, 'merge and grade the no-question-mark host surface lane').
next_action(2, 'make identity columns replace every at-sign salt rider and rerun extraction/host clock goldens').
next_action(3, 'merge target-neutral executor contracts and built-in extraction relation signatures').
next_action(4, 'execute the V6.2 exit audit in dependency order').
next_action(5, 'run pinned Grafana and file-watcher scale gates and record machine-readable baselines').
next_action(6, 'commit checkpoints, update ARCH task states, and push the branch').
