% Consultable session save (first of its kind, user idea 2026-07-29:
% "change save session to use pl so we can consult previous file").
% Sibling of the same-named .md; the .md is prose, this is the queryable
% spine. Load: swipl -q -l chat_log/<this file>.
% Query e.g.: in_flight(Name, Agent, Base, _), session_task(open, T),
% awaiting_user(W, _).

:- module(session_20260729_0, []).

session(id, '20260729.0').
session(topic, 'v6-overnight-p1-p4-complete-morning-struct-rulings').
session(main_sha, '0264d2b1').
session(battery, 'green-all exit 0: conformance 139/0, plunit 137/137, text_door 87/87/0, sweep 87/85/0 both modes, all receipts HOLD').
session(tag_gate, 'v6.2.0 gate (the bop) SATISFIED; push+tag = user').

% in_flight(Name, AgentKind, BaseSha, IntegrateNote)
in_flight(struct_as_rows, opus_worktree, '72d4d753',
          'header plans/2026-07-29-struct-as-rows-header.md; verify both edge-grade fixtures, sweep identical-only growth; unlocks 20 json fixtures + spans + lsp lines').
in_flight(extract_resolve_flag, codex_terra_shell_b3m6p2s1p, '0264d2b1',
          'worktree ../sprefa-codex-extresolve branch codex/extract-resolve, NO-COMMIT flow; review, commit, merge, then dispatch flow_interproc_port').

% landed(Arc, MergeNote) -- the night, in order
landed(prolog_org_refactor, 'all 10 review ranks, merge 9186f1ad').
landed(memory_soak, 'GET /stats + soak in green-all, merge a40b2dc6').
landed(watcher_buy_research, 'parcel>fs.watch>chokidar>watchman, 8b0b49a8; fs.watch shipped').
landed(extraction_live_p2, 'watch bind + enumerate hosts + extract host; (witness,ordinal) defect fixed; 8-phase receipt').
landed(edge_body_constructs, 'negation/binds/comparisons/now + edge-head typing; sweep 82/80; pre honestly refused').
landed(tick_phase_alignment, 'level freeze + departure frontier; flagship blocker removed; sweep 85/83').
landed(flagship_callgraph, 'callgraph-ast.dl vs v6 port, 0 unclassified diffs, rule-fidelity leg').
landed(cli_bop, 'commander serve/run/check/load/q, exit 0/2/1; uncommitted-done deviation caught').
landed(lsp_diags, 'rel diag_v5 IS the table src/lsp.rs:545 reads; real stdio both directions').

% session_task(open|user_owed, Description)
session_task(open, 'integrate struct-as-rows on landing (A4 law)').
session_task(open, 'integrate terra resolve flag; then dispatch flow_interproc_port').
session_task(open, 'phase 5: float REAL+avg, clock checker, ingest commit_ms').
session_task(open, 'small lanes: extra_drain_tick, emitter_groupby_literal, probe_output_guard, org_banked_findings, gen_staleness_gate, golden_flake_hunt').
session_task(user_owed, 'push main + v6.2.0 tag').
session_task(user_owed, 'parcel watcher dep word').
session_task(user_owed, 'v5 pile: orphan roots, lazy-rel tier, filesize rail, dom-match, worktree removals').

% awaiting_user(Item, Note)
awaiting_user(q8_residual, 'left-of-arrow = demand key confirmation').
awaiting_user(arms_lab_slots, 'consumption-arms SLOT pile').
awaiting_user(per_key_retention, 'keep(count) per-rel vs per-key ruling').

% ruled_this_session(Name, Value)
ruled_this_session(compound_storage, struct_as_rows).
ruled_this_session(smallest_correct_solution, standing_directive).
ruled_this_session(extractor_fixed_waiver, resolve_flag_expose_only).
