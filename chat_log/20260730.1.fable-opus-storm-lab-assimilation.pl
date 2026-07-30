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
