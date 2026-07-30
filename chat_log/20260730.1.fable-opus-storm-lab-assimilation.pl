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
:- discontiguous lane/4.
:- discontiguous lane_landed/3.
:- discontiguous finding/3.
:- discontiguous bug/3.
:- discontiguous directive/2.
:- discontiguous user_card/2.
:- discontiguous answered/2.

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
lane_landed(clock_checker_finish, 'merge ffcddfc7',
     'THE PIPELINE WORKED: sol implemented (A2/A4/A5/A7/A8/A9/A11 not_provable with pinned dependency/8 projections, battery exact), opus review gate FALSIFIED the one affirmative claim (A6 proven was a constants-vs-constants crosscheck; sabotage receipts S1/S2/S4), sol applied the 3-item fix list (nonzero-offset pipe crosscheck pinning the 4-tick delta sequence, clock_boundary negative case, ClockCatch pinning restored, A6 honestly renamed runtime_clock_crosscheck), coordinator re-ran plunit 197/197 and merged. Focused clock suite 24/24.').
lane(clock_checker_finish, codex_sol, dispatched,
     'USER DIRECTIVE (best intelligence): sol finishes the paused 3_clock_check.pl per clock_checker_resume_order (A2 stays not-provable, A6 inferred offsets vs runtime ticks, full battery); no-commit flow; then an OPUS 5 agent runs the review gate on sol output and coordinates feedback before coordinator merge. Prior opus lane stood down.').
lane(spot_check_assimilation_order, opus_readonly, dispatched,
     'review f8ab8ac5..HEAD commit-by-commit, re-run key receipts, inventory every open lab, return classification + optimal fold order').
lane(phase5_grade_and_fix, opus_worktree, dispatched,
     'grade landing_ready bool/float at tip; fix the two known_failures (serveHost.test.ts removed-syntax retention, 18 roundtrip fixtures); full battery').
lane_landed(json_v3v4_recovery, 'f9bf09df cherry-picked',
     'HEADLINE: the json language is NOT lost -- it is v5 at the repo root (src/datapath.rs brace-pattern walker, 5 generations, inner grammar stable since v1, NINE productions); v6 is the generation that dropped it. 22 constructs graded: 8 express today / 4 ugly / 6 behind storage cards / 5 need new surface, ALL 5 = the key axis (key as capture/regex/glob/descent vs decode exact-field). 9 cards for user, top leverage CARD-KEY-CAPTURE (unblocks 4/5), cheapest CARD-SUBTREE-CAPTURE (struct_as_rows already answered it structurally). Card evidence: residency=core_global, arrays=host_flattened, null=row_absence, schema=metadata_only, cycles=refuse. Also found: SYNTAX.md decode row stale vs registry; json_array/json_object refused despite ruled emittable (dispatch, not ruling).').
finding(phase5_agent_in_main_tree, process,
        'phase5 opus agent fell back to the SHARED checkout after worktree reap and committed 2ca13e37 directly on the lab branch; coordinator imposed scoped-add rules by message; two-writers-one-tree hazard noted').
lane(json_v3v4_recovery, opus_worktree, dispatched,
     'recover the v3/v4 json language surface+semantics from ~/projects/sprefa-archive-20260701 (and 20260428); grade against the 5 json_interop_lab cards; decision cards only, zero production syntax').
lane(file_span_reconcile, opus_worktree, dispatched,
     'reconcile file-span-design.md + file-identity-span-spine.md + locked single_rel_type_system + landed rel-ref runtime into ONE plan; implement only if zero cards remain open, else stop at plan').
lane(flow_parity_residue, codex_terra, dispatched,
     'classify every unmatched call-resolution target (v5only 87 / v6only 55), close flow_node_type + param_type residue, referee-side fixes only, 0 unclassified or named blockers').
lane_landed(comment_rail_wiring, 'merge 786b5daa',
     '6 of 7 verdict techniques as standing rails (arch markers, suppression, readme anchors, lang junctions, gen zones, lint+suppression-retraction) + parity referee; coordinator re-ran gate: RAILS HOLD, comment_node 745/745 v5-exact, arch_node 4/4; named skips = markdown grammar (extractor hole, standing) + technique-2 block pairing refused unbound_head_var').
bug(block_pairing_refusal_misnamed_suspect, codex_luna,
    'range-comparison body in the block-pairing rail refused as unsupported_construct(unbound_head_var(_)) -- same misnamed-refusal class as the = spelling (review-B4 family); needs a real name + location before the technique can be judged inexpressible').
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

% user directives 2026-07-30 afternoon (verbatim intent, rulings-grade)
directive(openapi_codegen_spine,
          'CLI + HTTP become proper OpenAPI codegen: ONE spec drives the ts node server now, rust axum later, and the CLI; the spec itself is GENERATED from prolog facts (dogfood the codegen from our language); this is the foray into json feature land').
directive(json_as_rel_type,
          'json becomes a rel column type that LOWERS TO SQLITE JSON1 ("we gotta play ball") -- REFINES the earlier one-rel-boundary reading that classified stored inline json as pure migration debt; typed refs and json columns coexist').
directive(json_syntax_native,
          'json natively expressable 1-1 in the language; unquoted keys when valid (json5-ish); HOLES like v3/v4 brace patterns; the { opening will later be abused beyond json, FOR NOW { means json').
directive(roots_never_again,
          'target = hundreds of repos AND an above-repos root; v5 roots concept must not return; defaults spelled as ordinary rels/builtins/sh/hosts; unspecified repo root = git rev-parse --show-toplevel walk; SCALE TEST EARLY').
directive(rust_flip_soon,
          'ts -> rust flip coming, prolog compiler stays; server + cli expressed from the same openapi spec on both targets').

% bug(Name, FoundBy, Text). -- every defect any lane surfaces, marked here on arrival.
bug(ref_pattern_depth2_silent_empty, filespan_lane,
    'relation pattern at depth>=2: emitted SQL json_extracts a path out of the INTEGER __id endpoint -- always NULL, rel permanently empty, NO refusal (join_column_type_mismatch catches only int columns by accident, path is text); measured oracle span=1/coord=1 vs emitter 0/0; root cause head_target_atoms/4 walks direct head args only; handed to rel_edge_clock_fixpoint lane, stage-1 fix = named refusal on ref columns').
bug(decode_depth2_oracle_derives_nothing, filespan_lane,
    'chained decode at depth 2: emitter produces the correct two-hop indexed join (1 row verified), oracle derives NOTHING -- compile_pattern_arg one-level implementation; same lane owns it').
bug(roundtrip_two_door_decl_disagreement, phase5_lane,
    'the 18 roundtrip fails = ONE defect: parse_dl:574 always carries type_decl AND col_type forms, print_dl:209 decl_ref_order prints a decl line per form = duplicate rel line on reprint, reparse arity-checks the doubled list and drops the type. Fix measured 19/19 + byte-identical views: 4-line print_dl guard + explicit col_type rows in 4_struct_values.pl, must land TOGETHER; coordinator owns the landing').
bug(a6_crosscheck_nondiscriminating, opus_review_gate,
    'sol clock checker A6 "proven" unearned: all-zero-offset graph = crosscheck compares two constants; sabotage receipts S1 (constant observer stays green) + S4 (Grade=0 hardcode stays green); clock_boundary multi-trigger requirement has no negative case (S2 invisible); fix list sent back to sol (nonzero-offset pairing proven by reviewer)').
bug(flow_call_target_resolver_defect, codex_terra,
    '32 call-target rows share a call coordinate but select DIFFERENT callees across engines (16 v5-only + 16 v6-only) -- real resolver divergence, distinct from the 110 extraction-input rows; per-edge attribution blocked by the serve-lifecycle flake below').
bug(serve_lifecycle_idb_read_race, codex_terra,
    'v6 HTTP server exited before final /idb reads in the flow rig -- paired TSVs lost after a full successful run; same family as the golden/EADDRINUSE flakes; unowned').
bug(hostdecode_hardcoded_port_collision, phase5_lane,
    'hostDecode.test.ts EADDRINUSE :::17611 when two lanes run the suite in one tree -- hardcoded test ports collide; passes isolated; unowned').
bug(run_results_json_mode_dependent, phase5_lane_and_review,
    'out/run-results.json refusal text differs naive vs incremental (engine.pl retract_from_log suffix) -- whichever sweep mode ran last leaves the checked-in receipt dirty; unowned small').
bug(phase5_untracked_view, phase5_lane,
    'dl_view/float_integral_value_keeps_real_storage.dl6 untracked -- bool/float lane never committed its regenerated view (174 tracked / 175 on disk); coordinator to commit').
bug(diag_rail_template_mismatch_suspect, coordinator_probe,
    'coordinator probe with diag-rail-shaped host decl (digest input unreferenced in template) refused template_mismatch(unreferenced_input(digest)) by bop check -- diag-rail.dl6 itself has that exact shape, so either the fixture no longer passes the text door or the doors diverge; VERIFY before next lsp-diags run').
bug(glob_root_level_zero_match, coordinator_probe,
    'watch glob **/*.ts matches NOTHING for repo-root-level files under git pathspec (files must sit in a subdir) -- known v5-globset-vs-git-pathspec divergence class, new bite: silent empty world, zero diagnostics; candidate for a load-time warning when a watch glob matches zero enumerated files').

% ── COORDINATOR FINAL THOUGHTS (fable, wind-down) ────────────────────────────
final_thought(review_gate_pipeline,
    'the sol-implements/opus-falsifies/sol-fixes loop earned its cost in one pass: the ONLY affirmative claim in the clock landing (A6 proven) was the only wrong claim, and only sabotage found it (all-green batteries reproduced exactly). Make this the default shape for any lane whose deliverable includes a PROVEN row').
final_thought(unit_maxing_confirmed,
    'every bug this session was a COMPOSITION bug under all-green suites (depth-2 refs both-directions broken, two-door decl interplay, misnamed refusals at construct junctions). The corpus grades constructs alone; cross terms are ungraded. golden_flex_e2e + its registry coverage gate is the structural answer; treat single-construct fixture additions as insufficient evidence of health from now on').
final_thought(harness_base_defect_recurring,
    'the agent-worktree base staleness class bit FIVE times in one session (stale cut x4, mid-run reap x2, fallback-to-shared-tree x1). Coordinator-cut worktrees at the exact sha + explicit-path staging is the only shape that held. Never dispatch Agent isolation:worktree in this repo again without checking the cut base').
final_thought(language_state,
    'locked core is real and holding (value plane, host surface, single rel model, storage); the open design mass is concentrated in three places: json/{}-family (directives now set, cards coming), scan/match composition surface, span-spine 3 residual cards. The one standing violation is the match-arm token pair. The doors and the registry are strong enough now that coverage gates can be GENERATED from the registry -- use that everywhere').
final_thought(dogfood_velocity,
    'the default-export diag went directive-to-live-rows in under an hour with zero engine edits (ast-pattern host + 5-rule program + served HTTP). The language is genuinely usable for its own tooling now; the parity/spelunk lane tests whether it can also MAINTAIN itself').

% ── WIND-DOWN 2026-07-30 (user out of tokens; agents left RUNNING) ─────
session_pause('2026-07-30', fable_wind_down_agents_alive).
pause_tip_note('branch codex/rel-ref-file-span-lab, tip = the comment-rails merge; run git log to find it; all landed work committed, ledger current').

% in_flight(Lane, Where, GateOnLanding). -- DO NOT redispatch; they report on their own.
in_flight(depth2_ref_fix, 'opus @ ../sprefa-lane-depth2 (lane/depth2-ref-fix)',
          'verify fail-first fixtures went red first + EXPLAIN SEARCH receipts; re-run full battery; merge branch').
in_flight(golden_e2e, 'opus @ ../sprefa-lane-golden (lane/golden-e2e)',
          'verify registry-coverage gate catches a removed construct (red receipt); re-run golden at 0/1/many + served leg; merge; then wire recipe into green-all').
in_flight(json_syntax_lab, 'opus @ ../sprefa-lane-jsonlang (lane/json-syntax-lab)',
          'CARDS ONLY -- no production syntax may land from this lane; relay the exact spelling card list to the user for sign-off (no-unsighted-syntax law)').
in_flight(openapi_codegen_lab, 'opus @ ../sprefa-lane-openapi (lane/openapi-codegen)',
          'verify parity-gate red/green receipt + emitted openapi.json validates; cards to user; merge lab as design record').
in_flight(v5_parity_spelunk, 'opus @ ../sprefa-lane-spelunk (lane/v5-parity-spelunk)',
          'verify parity table reconciles the scan()-105/129 receipt; git-diags rail red+green receipts; merge').
in_flight(flow_residue_partial, 'codex terra STOPPED, dirty tree @ ../sprefa-codex-flowres',
          'blocked on serve-lifecycle race (golden lane owns the fix); after that, resume terra session or fold its reconstructed tables; findings doc in that tree').

% next_coordinator(OrderedActions).
next_coordinator([
    read_each_lane_report_and_run_its_gate,
    merge_by_sha_only_after_gate,
    update_this_ledger_per_landing,
    cleanup_merged_worktrees('codex-clockcheck, codex-comrails, codex-flowres after fold; lane/* trees after merge; stray .claude/worktrees/clock-checker-resume + branch (draft 3eb59512 superseded by merged clock work -- verify then delete)'),
    rerun_crawl_bench_v6_leg_post_batching('grafana 40 f/s number is stale-pessimistic'),
    present_user_cards
]).

% user_cards_open (consolidated, present on next contact):
user_card(match_arm_tokens, 'ratify |-> |+> into rulings.pl vs revert 9cadb419 to arrow reuse -- the one standing syntax violation').
user_card(json_spelling_cards, 'json-syntax lab will return exact spellings (json5 braces, holes, list types) -- sign-off before any parser edit').
user_card(filespan_three, 'rev-family naming; line/col residency (in-language newline rows vs provider); work-revision identity').
user_card(fold_blocked_labs, 'scan_surface composition cards; json_interop module policy; select_scan_cache + generic_scan choices').
user_card(push_and_tag, 'main + v6.2.0 tag push remain user-gated; lab branch pushed to origin periodically by sol convention').

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

% ── USER DIRECTIVES 2026-07-30 morning (session resumed; agents were left alive) ──
directive(e2e_from_cli_never_ts_imports,
          'benches and e2e tests invoke the CLI (bop / the released binary) or nothing; no e2e test may import the codebase from TS. Any e2e that reaches inside the process is a cheating test. Receipts are TRACING MEASURES that come out the same way from prolog logs, ts node logs, and rust logs -- one receipt shape across all three targets, so the rust flip does not invalidate the receipt corpus').
directive(test_shape_min_test_max_coverage,
          'e2e minmax and integration minmax carry the grading weight; unit tests only for minor pure edge cases. Minimum number of tests for maximum coverage, every time. Explicitly banned: high-count low-value unit suites ("baby dick tests")').
directive(parity_where_it_counts,
          'parity is graded where it matters (real programs, real corpora, external oracles), not by unit-test volume').
directive(rust_flip_frontier_reading,
          'user reading of the trajectory: the only real remaining lab frontier is matching rx semantics FROM CODEGEN and weaving sqlite through it (mostly the same shape as today); then spelunk v5 for parity; then surpass it. Coordinator largely CONFIRMS from the pl files -- see answered(trajectory_check)').

lane_landed(openapi_codegen_lab, 'merge bf71c5b6',
     'coordinator re-ran receipts.sh itself, exit 0: parity gate 3/3 green, sabotage OPENAPI_LAB_DROP=readRelation 0/3 RED while the lying spec still passes Redocly (a validator cannot catch a lie by omission), withdraw -> 3/3 green. Four parity sources: emitted spec vs 4_http.ts ROUTE_LIST vs parsed dispatch branches vs a LIVE server 404 route list. Inventory 5 routes/5 operations/14 responses/14 schemas; CLI 7 verbs of which only 4 are routes (serve/run/check are process lifecycle, unexpressible in any spec-driven CLI generator = the ceiling on the CLI half). BUY: openapi-typescript (dev dep) + Redocly via pnpm dlx (no dep) + progenitor for rust clients post-flip; direct axum emission over utoipa/aide (utoipa makes rust the spec source and inserts a drift link); fastify-openapi-glue is the design to copy, rejected on architecture (adopting it = adopting fastify over a 410-line node:http front with soak-tested teardown). SSE honestly unspeakable in 3.1 (text/event-stream + type:string describes bytes not frames); 3.2 itemSchema is the fix; /ticks marked skip-for-codegen. Lab NOT deleted on landing: cards 6+8 would promote the prototype into serve/, so lab-death waits on that ruling.').
user_card(openapi_eleven, '11 cards from the openapi lab; load-bearing four: (1) spec artifact checked in + staleness gate vs gitignored out/; (2) info.version identity now written in a FOURTH place (git tag / package.json / facts / content hash); (6) static spec vs program-derived spec (GET /idb/{rel} is only typeable in the abstract -- rel and Row arity come from the LOADED program; a GET /openapi.json could expand per rel with real prefixItems, dogfooding twice); (8) does ROUTE_LIST become GENERATED from the facts (turns parity legs 1<->2 from a check into an identity, deletes the two-hand-kept-lists crack, but is the first production edit to serve/)').
finding(openapi_lab_cracks, defect_small,
        'serveFailure can answer with no body when headers are already sent (SSE case); the catch-all 404 {error, routes} shape is unspeakable in the spec; bop.ts casts JSON.parse(...) as {rows} unchecked on every read').

finding(grading_rig_is_in_process_everywhere, arc_sized,
        'directive(e2e_from_cli_never_ts_imports) indicts more than one lane: the WHOLE corpus grading rig drives the runtime by TS import. golden-run.ts (golden lane, 4bd4c38f) imports BootRunner/ScratchStore/TickLogEmitter/TickFold/rowValueFromSql from ../runtime, and it says so honestly -- it copies sweep.ts shape ON PURPOSE so a divergence is a module divergence not a two-copies-of-the-comparison divergence. sweep.ts and run-emitted.ts are the same shape. Target-neutral doors that ALREADY exist: the served HTTP leg (serve/main.ts, used by runtime-bridge phase 1) and bop run/q/ticks; the prolog side is already CLI-driven (golden_oracle.pl, golden_coverage.pl: 0 imports, swipl-invoked). CONSEQUENCE for the rust flip: the prolog leg and the served leg survive the flip, every in-process TS harness does not. Not a one-lane fix; wants its own arc with the receipt shape decided FIRST (one tracing-measure format out of prolog logs, node logs, rust logs).').

lane_landed(json_syntax_lab, 'merge 62f9ce84',
     'coordinator re-ran the lab entry itself: JSON_SYNTAX_LAB 25 PASS exit 0 (grammar 7, lowering 7, lists 7, cards 4); diff vs base = plans/ + labs/json_syntax/ ONLY, parse_dl.pl untouched, no-unsighted-syntax law held. HEADLINE: both constructs the recovery doc graded "genuinely needs new surface" have EXACT json1 lowerings -- key capture {$key:$value} is json_each(key,value) with zero new SQL, ** is json_tree, and json_tree.fullkey in the same join is v4s dropped $$${PATH?} for free. Only spelling is open. Grammar = 11 productions, literal grammar IS the pattern grammar minus holes (by construction, not a second DCG); key-axis productions are pattern-only forever (computed key = json_object(K,V)); quoting marks literals on the value plane, bareness on the key plane, FORCED by JSON5 not chosen. 27 archive examples parse verbatim; gh-cache flagship parses verbatim and its dl6 transcription yields the byte-identical IR. LISTS: json carrier wins 5/5 axes, list(T) = typed view over a json column, T over a closed 4-scalar set, checker delta measured at exactly 4 clauses, no type variable; 1000-element list = 1 row carrier / 1001 indexed / 1000 cons, all rendering byte-identically; relational element storage is not a list, it is a rel. CARD-SUBTREE-CAPTURE closes the projects oldest open json question (human-goals.md:693). 9 cards answered by the three ruled directives, 13 open carrying 29 exact spellings.').
finding(jsonb_not_portable, constraint,
        'jsonb is NOT portable across the two SQLite builds this project already runs (system CLI 3.43.2 rejects, libsql 3.45.1 accepts) so json columns store TEXT; REGEXP is not core SQLite (both current builds ship one, rusqlite does NOT -- prices CARD-REGEX-KEY against rust_flip_soon); json1 will not canonicalize so canonical_json_text/2 keeps that job and the cross-target tick-log contract does not move; the emitted e0.type=object guard is critical (without it, descending into a scalar raises malformed JSON and kills the statement instead of failing silently -- bit the lab on first run); SQLite can CHECK array-ness but cannot CHECK a list element type (no subqueries in CHECK)').
user_card(json_thirteen, '13 open json cards, 29 exact spellings, gated by receipt C3. Top 5 by leverage: (1) CARD-KEY-HOLE-SPELLING unblocks 4 of the 5 needs-new-surface rows -- {$key: $value} vs {(key): value} vs {[key]: value} vs invert-quoting; (2) CARD-PATTERN-GOAL-SPELLING decode(body,{..}) vs body = {..} vs match(body,{..}) -- NOTE the json_as_rel_type directive takes json as a TYPE word so v5s own json(body, q:{..}) op name is gone; (3) CARD-LIST-SPELLING tags: list(text) vs text[] vs json; (4) CARD-JSON-NULL now TWO questions (null inside json stored vs null as column value refused / reject at ingress / explicit variant); (5) CARD-BRACE-TAG reserve point{x:1} now for the stated later abuse of { or do not. Then JSON5-subset, string-quote, descent-depth cap, regex-key (priced against rust: rusqlite ships no REGEXP), value-pattern (lab recommends NOT wanted), format-dispatch; CARD-EDGE-BODY-JSON + schema_import_boundary are spelling-free scheduling only, and the latter may be INVERTED by openapi_codegen_spine (if the spec is generated from prolog facts, the import half may never be needed)').

lane_landed(external_oracle_scout, 'merge d9b808bf',
     'codex luna analysis-only, coordinator re-verified the headline counts itself (14 oracle test fns across 9 files, all #[ignore]d; scip.rs really does subprocess scip-typescript/scip-go/rust-analyzer). ANSWER to the user question: v5 YES -- oracle_madge.rs grades 8 relations (dep, cycle_member, orphan, leaf, summary, depends_on, npm_dep, skipped) from examples/madge.dl against madge --json/--circular/--leaves/--summary/--depends/--warning. v6 tsv2 legs: NO madge anywhere. CORRECTION to the coordinators own earlier claim that v6 is oracle-free: v6/sprefa-extract/tests/golden_parity.rs runs scip-typescript, scip-go and rust-analyzer as ORDINARY non-ignored call-resolution ratchets, so v6 external ground truth exists on the EXTRACTOR side and is absent on the ENGINE side. The real gap: no v6 resolved TypeScript dep(src,dst) relation to diff against madge; the extractor has no ts module resolver (types.rs:587-603), which is the actual missing piece, not the diff harness. v5 default-run count: 0 of 14, every oracle is on-demand behind a tool gate, so v5s external oracles are NOT gating either.').

% ── USER RULINGS 2026-07-30 (both written into v6/prolog/conformance/rulings.pl) ──
answered(json_key_hole_marker,
         'DOLLAR. {$key: $value} on both planes; unblocks 4 of the 5 needs-new-surface json constructs; lowering already proven json_each(key,value), zero new SQL').
answered(match_arm_tokens,
         'RATIFIED |-> and |+>. Authorship settled: the USER asked for them, for left-to-right reading (guards first, arrow, then head, so an arm reads the direction data flows). Standing intent attached: rel programs look uniform and express flow ACROSS TIME. finding(match_arm_new_tokens_unruled) CLOSED; the 23 migrated .dl6 fixtures stand').

answered(rx_operator_inventory,
         'MEASURED at 2026-07-30 HEAD. CODEGEN surface = 4 operators, and all 124 gen_emitted modules carry the byte-identical import line: concatMap, forkJoin, map, of. HAND-WRITTEN runtime+serve = 24 distinct: concatMap map of from forkJoin toArray filter catchError defer merge mergeMap expand finalize throwError scan take interval bufferTime fromEvent partition switchMap takeUntil tap share (plus EMPTY/Observable/Subject/Subscriber/asyncScheduler infra). switchMap appears ZERO times in emitted code; its only executing site in the whole app is 4_http.ts:399 accepted$.pipe(switchMap(runProgram$)) = program swap, and 2_binds.ts documents relying on it to close OS watches. CONSEQUENCE for rust_flip_soon: the emitted rx is a SEQUENCING SHELL (run statements in order, run independent reads together, shape rows, lift a constant); every datalog semantic lives in the emitted SQL text and the tick loop, so the port owes tickLoops expand + the serve-level lifetime operators, not a datalog runtime').
finding(test_shape_inventory, measured,
        'tsv2 tests at 2026-07-30 HEAD: 31 files, 88 test cases. 21 of 31 files import runtime/serve/lower internals directly (in-process); only 9 touch a served HTTP leg (startServed/fetch). Largest in-process files: structPlane 426, 6_host-extraction-batching 415, departureFrontier 259, serveHost 251, watchBootReconcile 227. The CLI-driven side is bopCheck/bopCommandInventory/bopRun (0 imports) plus bopLoadQuery. This is the concrete target list for directive(e2e_from_cli_never_ts_imports) + directive(test_shape_min_test_max_coverage)').

lane_landed(depth2_ref_fix, 'merge 2e2b983b',
     'coordinator re-ran EVERY leg on the branch including the fail-first: at 4b0bc279 (fixtures only) conformance is 176 PASS / 10 fail with the 6 relation_depth fixtures red on MISMATCH final file/2, span/3, dcoord/3, located/2 -- red before the fix, coordinators own run, not the agents claim. After: conformance 186/0, plunit 200/200, text door 131/131/0, roundtrip ALL GRADES, sweep both modes 131 compiled/129 identical/0 wrong (FINAL 129/2 pre-existing), tsv2 92 (91/1skip/0fail). bug(ref_pattern_depth2_silent_empty) and bug(decode_depth2_oracle_derives_nothing) CLOSED. Third divergence found en route that the plan had not measured: file disagreed at DEPTH 1 on bytes ({"name":"acme"} vs "repo(acme)") -- the tick log had been printing prolog term text for rule-built relation values all along, never graded.').
finding(memoization_invisible_to_end_state, count_test_law,
        'depth2 sabotage receipt: disabling the emitter relation-pattern memoization leaves EVERY conformance fixture and the WHOLE sweep green while the depth-2 span insert grows from 3 joins per arm to 5. End-state equality cannot see join growth; only the EXPLAIN hop-count test can. Third instance of the formerly-quadratic-paths-get-COUNT-tests law paying off, and the cleanest statement of why unit-maxing on end state does not catch composition cost').

directive(relation_pattern_stays,
          'user 2026-07-30, mid-lane: "dont listen to sol too much tho if its just destructuring in head positions that sounds like we would eventually want it". The landed relation-pattern feature is KEPT on its own merits; the sol adversarial review is demoted from keep-or-delete referendum to BURR FINDING only. Standing weight rule: an adversarial reviewer priced against a feature the user wants gets read for its defect list, not its verdict').
finding(coordinator_mislabeled_relation_patterns, coordinator_error,
        'the coordinator presented the depth2 relation-pattern feature to the user as "rels as values". It is NOT. It is nested construction + destructuring of ref-typed columns, first-order, rust-esque pattern syntax. The users rels-as-values means passing a REL ITSELF into an arg slot so a rule is parameterized by which relation it reads or writes -- higher order, needs call/N in prolog. User caught the conflation. Two lanes dispatched on it: lane/rel-as-value-lab (opus, labs the real idea + whether it moves the list(T)-only generics verdict + whether content id can name a REL and not just a row) and codex/relpattern-review (sol, adversarial on what landed)').

lane_landed(golden_flex_e2e, 'merge on codex/rel-ref-file-span-lab after 3f68c9c2',
     'coordinator ran the battery itself AND re-ran golden-flex on the merged tree beside depth2: green reached its last recipe (store 74/74), GOLDEN FLEX HOLDS exit 0, sweep 123 compiled/121 identical/0 wrong on the branch, conformance 186 merged. COVERAGE SABOTAGE was the coordinators own and it caught a coordinator error first: the initial attempt reported green because BSD seds 0,/re/ address silently made NO edit; redone with perl, file change confirmed by grep, gate exits 1 naming latest/1 unaccounted for, restore goes green. Lesson generalizes: a sabotage receipt that never verified the sabotage landed is not a receipt. golden-flex is wired into `green` itself, stronger than the gate asked for.').
finding(sabotage_must_verify_the_sabotage, method,
        'coordinator ran a coverage-gate sabotage that came back GREEN and nearly recorded it as a gate defect. The edit had never applied (BSD sed 0,/re/ is a GNU-only address form, no error, no change). Standing method fix: every sabotage receipt must PROVE the sabotage landed (grep the changed file, or diff it) before the gates answer is interpreted. A green result from an unapplied sabotage is indistinguishable from a blind gate').

directive(surface_blockers_immediately,
          'user 2026-07-30: "please for godsake bring this up immediately next time we are going lightspeed". Standing coordinator duty: open design cards and FOLD-BLOCKED labs get surfaced at the TOP of a status report, not on request. A blocked lab is idle capital. Current biggest single unblock: scan_surface_composition (one user decision frees 3 labs: scan_surface_composition, scan_match_reconciliation, generic_scan_instantiation, and select_scan_cache rides the same axis)').
finding(labs_not_assimilated, standing,
        '10 labs alive on disk against a lab protocol that says labs die on landing. 2 deliberately alive (json_syntax, openapi_codegen -- their cards would promote them). 8 are debt: rel_value_unification (12 files, BIGGEST, and it is the same territory as the users rel-as-value question -- pointed the running lab at it), ghcacher_tick_golden (7), rel_definition_hash, json_interop, scan_surface_composition, scan_match_reconciliation, generic_scan_instantiation, select_scan_cache').

% ── DESIGN THREAD 2026-07-30 (user + coordinator, OPEN TENSION, not rulings) ──
% design_tension(Name, Text). Nothing here is settled; two labs are grading it.
:- discontiguous design_tension/2.
design_tension(identity_vs_flattening_conflated,
    'user: we over-indexed on unique keys. IDENTITY (which row wins) is answered by keys and normal forms and is solid. FLATTENING (how many inners run, what happens to losers) has NO spelling and is hardcoded to concat. distinctUntilChanged(byKey) is not switchMap; switchMap flattens every matching source event and tears down losers. Two axes, one mechanism').
design_tension(effect_plane_deaf_to_retraction,
    'measured by the rxoracle harness: HostRunner.liveDemand$ reads delta.add and NOTHING anywhere reads delta.del. There is no site at which a teardown could fire. Consequences measured: superseded inner runs to completion, its answer lands durably and is memoized, and concatMap makes the loser BLOCK its successor for the losers full duration. sprefa does not cancel, it memoizes').
design_tension(nothing_has_a_stream_type,
    'a rel is a TABLE, the tick is the ONLY time axis, deltas are derived per tick. A stream cannot be named, bound, passed or returned. Every rel HAS a delta stream; no rel IS one. Consequence: switchMap cannot be hosted as a construct whose result is a new stream, because there is no slot for a stream to land in; its result would have to be a new REL, which returns the question to tables').
design_tension(two_doors_for_flattening,
    'DOOR 1 the language expresses it: delta.del reaches the effect plane, an ordinal makes newest-wins ordinary data, switch becomes a rule (retract demand rows not at the latest ordinal). Cost: nothing new in the type system. |-> arms + finalize + keyed replace already carry most of it; the missing piece is mechanical. DOOR 2 host switchMap as a construct: needs a stream type or a rel-that-is-a-stream, which fights locked(single_rel_type_system). Lanes lane/teardown-flatten and lane/rel-as-stream are grading both').
design_tension(scan_forces_a_state_model,
    'user objection: scan sucks because it forces a state model. Measured in rxoracle scan_state_feedback: events +1 +1 +2 give rxjs 1,2,4 (every intermediate an event) and sprefa the correct total 5 with the intermediate 4 COMPUTED and CONSUMED and not an event anywhere. Datalog hands you STATE, not the sequence of states. Getting the sequence means making it a rel (log + ordinal), which is the honest lowering rather than a workaround').
design_tension(tier_zero_test,
    'the standard the user set for any new syntax: classify every wanted construct as (a) sugar over an existing lowering, (b) a new lowering of existing semantics, or (c) TIER 0, a genuinely new semantic no lowering over current constructs can express. Only (c) justifies syntax. Prove tier-0-ness, never assert it').
design_tension(north_star_reading_order,
    'user standing intent: the language should eventually read left to right, top down, point free, FRP/rxjs flavored. |-> and |+> were ratified 2026-07-30 for exactly this reason. A spelling only a compiler author can read is a wrong answer even when it lowers correctly').
design_tension(no_dot_syntax_on_rels,
    'measured: zero registry rows, zero SYNTAX.md mentions. Two accessors exist and do the same job -- the relation pattern (located(span(file(_, fpath(P)),_,_),_)) and the decode chain. A dot would be a THIRD spelling for the same access. Card, not a gap').

% ── WAVE 3 LANDINGS 2026-07-30 (all coordinator-verified by re-running receipts) ──
lane_landed(rx_oracle_harness, 'merge on the lab branch',
     'coordinator ran run.sh itself: RXORACLE HOLDS 8/8 as declared. Leg A literal rxjs importing nothing from the repo, leg B bash only (bop serve + curl). Clock is the STEP not the tick; an event within guardMs of a boundary fails LOUDLY as a straddle, and that guard caught the agents own driver drifting 40ms/step. N3 (drop sprefas del lines) is opt-in because OPTING IN IS THE FINDING. CANCEL ANSWER: sprefa does not cancel, it MEMOIZES; the loser runs to completion, its answer lands durably, re-demand is a same-tick cache hit where rxjs re-subscribes and re-runs. Discriminating outside observation = TIME the re-demand').
lane_landed(teardown_flatten_lab, 'merge d966b407',
     'coordinator ran receipts.sh: TEARDOWN LAB HOLDS, 15 receipts. HEADLINE: door 1 was ALREADY THE RULED POSITION and has been unimplemented since 2026-07-27 -- effect_abort needs no amendment, its own text says demand-row deletion IS the abort signal and names the unbuilt site (1_hosts.ts:387 filters inserts only). The kill is already written at 1_hosts.ts:175 and has NEVER been called. Four flatteners measured on the same /ticks stream with one operator changed; concat reproducing the shipped ledger exactly is what makes the other three credible. THREE FINDINGS PAST THE BRIEF: merge and concat are the SAME PROGRAM (which inners die = programs business, how many run = machines); switch needs NO slot key because del already carries the rules decision, making teardown-on-del strictly MORE GENERAL than switchMap; NO ORDINAL is needed anywhere (three placements priced and rejected -- the coordinators earlier sketch was wrong on that). Teardown = 4 sites in ONE file, all serve/1_hosts.ts. Lab placed at repo-root labs/ not v6/prolog/labs/, noted not moved').
lane_landed(rel_as_stream_lab, 'merge 7d040ab0',
     'coordinator ran receipts: 10 PASS 0 FAIL. TIER-0 LIST IS EMPTY: 19 constructs classified, one (a), two (b), rest already spelled; no new syntax justified. A stream is a VIEW OF A TABLE PLUS A NAMING CONVENTION, and locked(single_rel_type_system) is not fought anywhere because log_on_level_headed_rel already forces a readers projection to be a table. DECISIVE RECEIPT: one tick batching two increments, keyed cursor reports 1->3 with the intermediate gone, log rel reports ordinals 2 and 3 as separate rows -- the users scan objection is exact and the fix is not removing the state model but PUBLISHING THE SEQUENCE BESIDE IT. Also: backpressure with ZERO constructs (writer gated on a derived watermark, overflow visible in a dropped rel); keep(count(0)) is deliver-and-forget so the whole rx Subject family is a two-word decl matrix; zip = equijoin on the ordinal, bufferCount = integer division; the ordinal survives a process restart and rx has no durable position. CRACK: the two doors already carry DIFFERENT internal orders (engine.pl:357 one counter across all log rels per tick vs lower.pl:2275 per-table rowid), neither graded because neither is observable; surfacing an engine ordinal must pick one and the pick becomes a graded contract').
lane_landed(prolog_compile_profiling, 'merge 4dd7e230',
     'THE 232s MYSTERY IS NAMED. plan phase = 255,333ms of 255,490ms on flagship-flow.dl6 at 6,011,087,004 inferences; hot predicate clock_check:graph_reachable/4 under clock_scc/3; local growth exponent 4.899 (26ms at 10 rules, 86,742ms at 58). Coordinator read the code and confirms the story: SCC computed by ALL-PAIRS MUTUAL REACHABILITY where each call enumerates SIMPLE PATHS with a Visited list and no memo. Sharpest number: a 27-rule REAL program took 255s while a 30-rule chain took 3.6s, so graph SHAPE dominates size. Method (the user asked to be told how the numbers were obtained): statistics/2 deltas inside setup_call_cleanup/3, library(prolog_profile) for predicate attribution (library(prof) unavailable); prolog_trace_interception/4 REJECTED at a measured 5.3x overhead at 20 statements; debug/3 rejected as messages not counters; /usr/bin/time -p with 2 repeats because hyperfine is absent. Off path proven free by sha256 equality of control/off/on, not asserted. plunit 200/200').
lane_landed(v5_parity_spelunk, 'merge 754889b5 -- LATE, see the process finding',
     'v5 is SELF-DESCRIBING (op_catalog/fn_catalog/rel_catalog are builtin rels) so the inventory comes from ASKING v5 through an sh host and the line-regex leg is graded against that. 28 ops / 16 fns / 112 rels; v6 covers 20, partial 4, ABSENT 132, and ZERO of sixteen scalar functions. Highest-usage absent: gen 24 files, comment 20, closure 17, scc 9. scan() 105/129 reproduces EXACTLY and is the shell-glob spelling (the programs 108/132 is git-pathspec where * crosses /); nothing was stale; scan is in 29 of 33 .dl rails, which is the number with weight. LIVE SHIPPED DEFECT found: serve/1_hosts.ts parseWhitespace reads N lines as one row of N columns when line count equals column count, so the shipped 2-column enumerate_at SILENTLY MANGLES any 2-file answer (right at 3, wrong at 2) -- now defect D1. Also: a backslash in a .dl6 string constant is silently deleted TWICE (parse_dl.pl:301 catch-all, then JS template-literal processing) so every regex in a dl6 program is backslash-free by necessity -- now D2').
finding(coordinator_forgot_to_merge_a_landed_lane, process,
        'the v5-parity-spelunk lane reported done, the coordinator relayed its findings to the user IN FULL, and then never merged it. Caught only when the user asked "did we scaffold v5 btw". The relay is not the landing. Standing fix: a lane is not done until its branch is merged, its worktree removed, and its row written here; reporting findings is step 3 of 4, not the last step. filespan-reconcile had the same shape (1 commit, plan doc, merged at the same time)').
finding(merge_receipt_is_not_discriminating, receipt_gap,
        'user caught it: the teardown labs merge receipt reads start r1 / start r2 / done r1 / done r2, but BOTH JOBS HAD THE SAME DURATION, so completion order matching start order is consistent with concat-like behaviour and proves nothing. The discriminating fixture is unequal durations (slow t1, fast t2) where merge must show start t1 / start t2 / done t2 / done t1. Out-of-order completion is the only proof. Same class as sabotage_must_verify_the_sabotage: a receipt whose green is also green under the wrong hypothesis is not a receipt').

answered(scan_surface,
         'RULED no_new_surface_match_block_arms. Tier-0 list came back EMPTY, so no new syntax is justified; the canonical scan is a keyed state rel + a log rel + a match block of |+> arms, all shipping constructs, graded byte-identical on both doors by the rel_as_stream lab. Consequence: the scan_surface_composition decision that was blocking FOUR labs is ANSWERED -- scan_surface_composition, scan_match_reconciliation, generic_scan_instantiation and select_scan_cache can all fold, with generic_scan_instantiation held only for the rule-generics card per the rel-as-value labs card 6. Sugar comes later FROM EVIDENCE (repetition ugliness), not from a guess. Still open under it: the ordinal spelling card (four rules today / seq() column sugar / engine-minted, the last one carrying the two-doors ordering crack) and loop/select mechanics, which the user wants nearby-and-intuitive for lowering.').
answered(json_edge_body_refusal_reason_is_stale,
         'lower.pl:931 keeps edge_body_needs_json_destructure with the stated reason that a compound value ARRIVING into an untyped column is stored as canonical term text and that the encoding question belongs to SLOT-TERM-STRUCT. SLOT-TERM-STRUCT WAS RULED 2026-07-29 (compound_storage = struct_as_rows). The refusal is parked on a slot that no longer exists. Supports the users argument that json is not a time thing and edge bodies should accept decode; the guard seam already landed for negation, comparisons and binds').
answered(json_literal_brace_tag_shape,
         'user proposed SWIs untagged dict spelling _{..} for json literals. What it buys: Tag{..} falls out by the same convention, so the brace-tag card is answered by convention instead of a separate decision. What it costs: the json5 directive asked for json natively expressible 1-1, and {a:1} pastes out of a json file while _{a:1} does not. Coordinator recommendation, unruled: accept BOTH, bare {..} and _{..} both untagged, Tag{..} reserved -- one grammar alternative, keeps paste-ability, reserves tags').

lab_folded(scan_surface_composition, '96428913', 'superseded by ruling(scan_surface, no_new_surface_match_block_arms). Its 10 receipts proved a scan_definition/1 expands N->N-1 into ordinary rules using named rels, keys, match, pre/1, latest/1, <- and <+; the ruling says we write those rules directly instead of minting the definition form. Recover: git show 96428913:v6/prolog/labs/scan_surface_composition/0_receipts.pl').
lab_folded(scan_match_reconciliation, '96428913', 'superseded by the same ruling. Its 10 receipts proved nested match composes through an ordinary rel, a direct nested block refuses, and scan+match expands to ordered rules; the shipped canonical spelling in labs/rel_as_stream/match_stream.dl6 is the same shape, graded on both doors. Recover: git show 96428913:v6/prolog/labs/scan_match_reconciliation/0_receipts.pl').
lab_folded(select_scan_cache, '96428913', 'superseded by the same ruling plus the consumption-arms verdict (log + keyed cursor + min composes N readers). Its merged-Log-as-select-queue receipt and durable Seq witness are the select mechanics the user asked about, and they need no new construct. Recover: git show 96428913:v6/prolog/labs/select_scan_cache/0_receipts.pl').
