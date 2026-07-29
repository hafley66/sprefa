% Consultable session save (second of its kind; convention set at
% 20260729.0). The same-named .md is the prose sibling; this is the
% queryable spine. Load: swipl -q -l chat_log/<this file>.
% Query e.g.: in_flight(Name, Agent, Base, _), held_branch(B, _, _),
% awaiting_user(W, _), ruled_this_session(R, _).

:- module(session_20260729_1, []).

session(id, '20260729.1').
session(topic, 'v6-lane-storm-struct-landed-key-order-ruled').
session(main_sha, '01ac896e_pre_compact_save_commit_follows').
session(pushed_through, '01ac896e').
session(battery, 'conformance 156 PASS (just-conformance form; bare swipl go.pl is VACUOUS), sweep both modes 96/94/0-wrong, TEXT_DOOR 96/96/0, plunit 137/137, roundtrip ALL PASS, tsv2 68-69/1skip, dl 96/1skip, store 74/74, staleness gate OK, extraction-live + leak-soak HOLD').
session(machine, 'v5 daemon STOPPED (launchctl bootout gui/501/com.sprefa.dl; restart = launchctl load ~/Library/LaunchAgents/com.sprefa.dl.plist); 3 hung dl --lsp killed (~4h each, one leaked per lsp-diags run)').

% landed(Name, MergeShaOrNote)
landed(extract_resolve_flag,      '17778bbb + ledger c26b4e0e (terra); flow_interproc unblocked').
landed(watcher_dep_ruling,        '98bc54a1: fs_watch_until_bench_regression').
landed(extra_drain_tick_fix,      '540e876c: reconcile re-INSERTs no longer frontier-staged; fail-first extraDrainTick.test.ts').
landed(flow_interproc_scout,      '749f1be5 (luna): gap map, df aux = P0, closure = direct recursion').
landed(flow_portable_port,        'd322c93f (terra): callable-plane approximation + rig, honest stops').
landed(staleness_gate,            '24d1bb0d (sonnet): caught 2 REAL staleness incidents same day').
landed(golden_flake_rca,          'e2a71856 (sonnet): 1/18 repro, zero payload, native signature; diagnostics; NEW reactor_buffertime_flake row 6/18').
landed(extract_df_aux,            'fb7c362a (luna): df param/arg all 4 langs, kotlin resolve, flat sig owner, caller_site spans').
landed(watch_boot_reconcile,      'ea938d6c (sol): delete-while-down retracts; A12 one-shot crossing sanctioned').
landed(struct_as_rows,            'dcfa6fcc + 9aef1633 (opus, 6 commits): type decls, dictionaries, decode->join, spans-as-struct, both edges graded byte-identical').
landed(struct_key_order_ruling,   'd5972a7e: canonicalize_world_rows/3 in 0_type_plane threaded through run_program; keys_not_sorted dead; fixture flipped, +1 compiled +1 identical').
landed(codex_orighead_rootcause,  'coordinator-cut worktree git metadata outside codex sandbox; read-only rev-parse base check + no-commit flow; codex-delegate.md updated').

% landed after the first save (this file updated pre-compact):
landed(compiler_quality_bundle,   'merge 733d4c1b (sol): groupby (N+0) 8 diag modules moved, probe guards = GOAL PLACEMENT fix in host expansion, 4 drift pins flipped, 0_refusal_messages.pl umbrella over 77-signature inventory + coverage test; battery conformance 158/plunit 138/sweep 98-96-0 both modes/TEXT_DOOR 98/LSP HOLDS; location residue = rule-index unavailable (parse_dl keeps no positions)').

% in_flight(Name, AgentKind, BaseSha, IntegrateNote)
% AGENT ROSTER AT COMPACT TIME -- each reports via task notification;
% coordinator reviews, re-runs battery, commits, merges, cleans worktree.
in_flight(host_output_seam, codex_sol, '13fef29c',
          'worktree ../sprefa-codex-hostseam branch codex/host-seam, NO-COMMIT; compiler acceptance for declared-struct host output columns + serve plan-union widening + StructPlane intern; brief plans/2026-07-29-host-output-seam-brief.md; ON LANDING: merge, battery, THEN merge held branch codex/flow-parity (ccfe53ec) + produce four-query parity table, THEN dispatch phase 5 (pre-authorized)').
in_flight(crawl_bench, codex_luna, '67719352',
          'worktree ../sprefa-codex-crawlbench branch codex/crawl-bench, NO-COMMIT; grafana org-fan v5 leg vs v6 served extraction leg, nice -19, NOT green-all; v5 yardstick 42739 files/389 repos/5.9s; brief plans/2026-07-29-crawl-bench-brief.md; verify luna output HARD (user: sonnet-class, only slightly better)').
in_flight(simplify_review_x4, opus_subagents, '01ac896e',
          'FOUR read-only opus reviewers over git diff 934dcc4d..HEAD (generated files excluded), one angle each: reuse / simplification / efficiency / altitude. First sonnet wave STOPPED on user word (opus only for the polishing stone). On each notification: collect findings; when all four in, dedup, apply surviving fixes in main tree, re-run affected suites, commit. User directive: simplify wherever possible.').

% held_branch(Branch, Sha, BlockedOn)
held_branch('codex/flow-parity', 'ccfe53ec',
            'struct_host_output_seam: span-typed host outputs refuse column_type_wrapper(_,_,none) at compiler AND serve/1_hosts.ts stringifies structs; seam compiler half shares files with the RUNNING cq-bundle lane -- dispatch seam arc after sol lands, then merge this branch and produce the four-query parity table').

% queue after sol lands, user pre-authorized ("dispatch the remaining work")
session_task(open, dispatch_struct_host_output_seam_after_sol).
session_task(open, merge_flow_parity_after_seam).
session_task(open, dispatch_phase5_after_sol).   % float/REAL+avg, clock checker, commit_ms
session_task(open, push_3_unpushed_commits_on_word).

% awaiting_user(Slot, Note)
awaiting_user(slot_term_struct,
              'undeclared compounds stay refused non-structs (no decl = no field mapping). Coordinator analysis relayed: DECLARED-compound arrival could canonicalize positionally via the decl (same induction logic as the key-order ruling) and the 9 temporal_pipe fixtures migrate by adding decls -- awaiting bless').
awaiting_user(v620_tag_timing, 'tag local; user: no tagging a non-working thing').
awaiting_user(daemon_restart, 'stopped this session for machine reclaim; also open: orgs-root watching model').
awaiting_user(v5_pile, 'orphan roots rm ~1.86GB; rel_port_of_reach VACUUM; lazy-rel tier; filesize rail; dom-match rewrite; worktree-salvage README commands').

% ruled_this_session(Ruling, Value)
ruled_this_session(watcher_dep, fs_watch_until_bench_regression).
ruled_this_session(struct_arrival_key_order, decl_induced_canonicalize).

% process_lessons(Name, Note)
process_lesson(tasklist_empty_not_proof_of_death, 'struct agent survived /clear; duplicate resume agent launched into its worktree, stopped with zero writes; check worktree mtimes/commits first').
process_lesson(vacuous_conformance_form, 'swipl go.pl without -g go prints nothing and greps pass vacuously; justfile recipe only').
process_lesson(lsp_diags_leaks_process, 'v5_lsp_exit_hang leaks one hung dl --lsp per receipt run; ESCALATED in ARCH; interim pkill owed in lsp-diags.sh').
