% Fable coordination session: opus storm + lab assimilation + json language recovery.
% Load:
%   swipl -q -l chat_log/20260730.1.fable-opus-storm-lab-assimilation.pl
% Queries:
%   directive(Name, Text).
%   lane(Name, Owner, State, Deliverable).
%   answered(Question, Answer).
%   finding(Name, Severity, Text).
%
% THE COORDINATOR'S STANDING JOB THIS SESSION (user-set): keep this file
% current, review/eagle-eye every landing, call things out. Update on every
% lane merge and every finding.

:- module(session_20260730_1, []).

session(id, '20260730.1').
session(date, '2026-07-30').
session(branch, 'codex/rel-ref-file-span-lab').
session(base_at_open, 'c6e2bf7b').
session(prior_ledger, 'chat_log/20260730.0.v6-2-ts-closeout.pl').

directive(agent_mix,
          'opus heavy (user limits reset ~5h, spend now); codex luna/terra for briefed lanes; sol only under coordinator self-doubt (fable-level, expensive); avoid sonnet (rots the codebase)').
directive(no_new_syntax,
          'absolutely none; new-syntax pressure follows the locked stop rule: lab the fork, plan with costs/receipts, return for user ruling').
directive(spot_check_everything,
          'every landing since f8ab8ac5 gets reviewed; claimed receipts re-run, not trusted').
directive(assimilate_labs_optimal_order,
          'every open lab classified and folded per the receipt_folding ruling; ordering delegated to opus review').
directive(sprefa_extract_policy,
          'reasonable CLI updates allowed for parsing logic and output composition ONLY; never dl6-specific; no vN binding; important machines live in prolog and lower into any target').
directive(json_language_goal,
          'built-in json query language wanted as structured query surface (graphql-adjacent); recover the v3/v4 json language design from the archives; that surface was the taste target ("ultra tight")').
directive(ledger_duty,
          'this file stays current through the session; it is the main coordinator deliverable alongside review').

% lane(Name, Owner, State, Deliverable).
lane(clock_checker_finish, codex_sol, dispatched,
     'USER DIRECTIVE (best intelligence): sol finishes the paused 3_clock_check.pl per clock_checker_resume_order (A2 stays not-provable, A6 inferred offsets vs runtime ticks, full battery); no-commit flow; then an OPUS 5 agent runs the review gate on sol output and coordinates feedback before coordinator merge. Prior opus lane stood down.').
lane(spot_check_assimilation_order, opus_readonly, dispatched,
     'review f8ab8ac5..HEAD commit-by-commit, re-run key receipts, inventory every open lab, return classification + optimal fold order').
lane(phase5_grade_and_fix, opus_worktree, dispatched,
     'grade landing_ready bool/float at tip; fix the two known_failures (serveHost.test.ts removed-syntax retention, 18 roundtrip fixtures); full battery').
lane(json_v3v4_recovery, opus_worktree, dispatched,
     'recover the v3/v4 json language surface+semantics from ~/projects/sprefa-archive-20260701 (and 20260428); grade against the 5 json_interop_lab cards; decision cards only, zero production syntax').
lane(file_span_reconcile, opus_worktree, dispatched,
     'reconcile file-span-design.md + file-identity-span-spine.md + locked single_rel_type_system + landed rel-ref runtime into ONE plan; implement only if zero cards remain open, else stop at plan').
lane(flow_parity_residue, codex_terra, dispatched,
     'classify every unmatched call-resolution target (v5only 87 / v6only 55), close flow_node_type + param_type residue, referee-side fixes only, 0 unclassified or named blockers').
lane(comment_rail_wiring, codex_luna, dispatched,
     'wire the comment-node verdict techniques into production rails, graded like the lab (745/745 target); fold per receipt_folding').

% Deferred to wave 2 (pending spot-check ordering):
queued(rel_definition_hash_fold, 'lower.pl/compile ownership overlaps wave-1 lanes').
queued(mode_scope_proof_views, 'needs ARCH row re-read before dispatch').
queued(scan_surface_composition, 'fold-blocked on indexed user surface decision').
queued(json_interop_fold, 'fold-blocked on indexed optional-module policy; v3/v4 recovery lane feeds the ruling').

answered(sprefa_extract_usage,
         'v6.2 spawns the released binary via executor contracts (2 executor keys: shell, sprefa_extract; 14 extract declarations); batching landed (same-tick demands grouped, one stdout fans into typed response rels, flow 7->1 subprocess/path); extractor additions stay typed-rust-side (ast_pattern_query = AstPatternQuery + flat AstCaptureFact, zero DL types in the extractor); locked contracts already match the user policy (sprefa_extract_scope, extractor_trait_boundary, parser_residency); gaps: document_sources (no md/html/xml/toml/yaml Source), file_blob_repo_revision (BlobSource unimplemented, Repo/Rev types absent); named overuse: grafana 40.26 f/s under per-witness subprocess boundary').
answered(lang_design_lock_state,
         'LOCKED: single rel type model + one checker, host = ordinary RHS atom (? and @ removed), parser residency rust, surface freeze + stop rule, target-neutral plan order, bool/float storage, clock-checker scope. OPEN CARDS: scan/match composition (4 questions + 3 surface cards), json residency/arrays/null/schema/recursion (5 cards), span-spine decision rows, assign 3 cards, decl-legibility cluster, older slot pile. Verdict: value plane + host surface + storage locked; composition surface and json/module/span identity are the remaining design').

% finding(Name, Severity, Text). -- appended as review proceeds.
finding(match_arm_new_tokens_unruled, syntax_violation,
        'commit 9cadb419 minted |-> and |+> as NEW text-door tokens for guards-first match arms (prior match_arm reused <- / <+ head-first); rulings.pl byte-unchanged across the branch, only authority is an unattributed surface_ruling/2 in the agent-written completion-drive plan WHICH FORBIDS new spellings 8 lines earlier; term form unchanged, 23 .dl6 fixtures hard-migrated; USER CARD: ratify into rulings.pl vs revert to arrow reuse').
finding(bop_load_query_flake, defect_unowned,
        'tests/bopLoadQuery.test.ts load-then-q fails under full-suite load, passes 6/6 isolated -- port/contention flake class, indexed nowhere; second undisclosed failure beside the known serveHost one').
finding(branch_gate_red, standing,
        'just green is RED on this tip: 18 roundtrip failures all 4_struct_values fail(not_variant) (owned by phase5 lane in flight); ledger counts stale in the green direction (plunit actually 188/188 vs claimed 173, text_door 123/123 vs 122)').
finding(spot_check_clean_bill, cleared,
        '16/25 commits clean; eprintln net 109->109 (reflow only); zero dep additions; no second type system (9a245a2e adds no runtime interfaces; extractor adds 2 typed structs per trait boundary); no new magic names; probe/salt removal genuinely complete in tracked source; full sweep regenerates byte-identical (no gen staleness debt)').
finding(fmt_residue, trivial,
        'cargo fmt --check fails on src/setup/hooks_tests.rs (one stray blank line) -- 4afb949b incomplete; coordinator fixing').
finding(arch_stale_probe_rider, trivial,
        'ARCH.pl construct(probe_rider, t5, new) survives the probe-surface removal; coordinator fixing').
adopted(fold_order_wave2,
        [fix_red_gates_first, rule_on_match_tokens, kill_folded_labs,
         wire_ghcacher_gate_post_luna, fold_mode_lab_views,
         clock_then_rel_definition_hash_serial, comment_fold_post_luna,
         flow_fold_post_terra, park_card_blocked_labs]).
finding(clock_opus_standdown_bank, handoff,
        'stood-down opus clock agent banked UNVERIFIED draft 3eb59512 (branch clock-checker-resume, self-minted worktree .claude/worktrees/clock-checker-resume after its assigned tree was reaped -- disclosed deviation, tree+branch pending cleanup after reconciliation). SOLID probe findings for the sol reviewer: all 8 historical programs ACCEPTED by check_clock_program/1 (not_provable rows are honest); A4/A7/A8/A9 decl-only = empty causal graph; A5 same/1 vs same/2 are distinct Name/Arity clock nodes so the emitted-identifier collision is invisible by construction; A6 zero-offset data alone is NON-DISCRIMINATING (hardcoded offset-0 would pass) -- pair with edge_chain_hops_tick_per_stage (inferred stage_two offset 1, observed tick 2 vs origin 1) to make the crosscheck falsifiable. Pre-edit baseline plunit 188/188.').
finding(worktree_base_staleness_recurrence, process,
        'Agent-tool worktrees cut from stale local main (01ac896e, 73 behind base 6c3a7e2d) -- the 2026-07-28 process-defect class again; all 4 worktree opus lanes hit it; clock + phase5 agents STOPPED correctly per dispatch law; coordinator resumed each with the sanctioned ff-only merge to 6c3a7e2d, commits land on agent branches, merge-back by sha').
% stale_in_claude_md(Claim, Correction). -- auto-loaded context now contradicted by the world.
stale_in_claude_md('hosts wiring presents `? probe` and `@ salt` riders as landed surface',
                   'both REMOVED by the v6.2 host-surface locks (no_rhs_probe_marker, no_salt_rider); a registered host relation is an ordinary RHS atom').
stale_in_claude_md('main is the ride with 32 unpushed commits',
                   'the ride is codex/rel-ref-file-span-lab, 40+ ahead of local main; origin/main further behind').
stale_in_claude_md('phase 5 (float/REAL+avg, clock checker) is the remaining unstarted leg',
                   'bool/float LANDED (landing_ready + verification in 20260730.0 ledger); clock checker paused mid-implementation with recorded resume order').
stale_in_claude_md('fresh-worktree js packages runnable after checkout',
                   'pnpm install required per worktree per package (v6/tsv2, v6/sprefa-store/js, v6/dl); bit the terra lane; binary flavor previously ledgered as gen_staleness_gate').

finding(isolation_worktree_reaped_midrun, process,
        'harness auto-cleans unchanged agent worktrees; a lane that STOPs clean at the base check is exactly that shape, so its resume finds no tree (file_span lane hit it). Mitigation adopted: coordinator-cut worktrees at the correct sha (git worktree add ../sprefa-lane-<name> -b lane/<name> 6c3a7e2d) + non-isolated agents pointed at them; file_span relaunched that way on lane/filespan-reconcile').
finding(flow_rig_no_binary_override, defect_small,
        'flagship-flow.sh hardcodes $REPO/target/release/dl with no env override (extract has DL_EXTRACT_BIN); codex terra lane stopped clean on missing binaries after 1/6 runs; coordinator building both release binaries and copying into the codex worktree, then resuming the terra session').
