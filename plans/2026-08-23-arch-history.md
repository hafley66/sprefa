# ARCH task history

Comments moved from `v6/prolog/ARCH.pl` on 2026-08-23. Entries retain the source text and original line number. This file is append-only.

## surface_dcg

Source: `v6/prolog/ARCH.pl:690`

```text
LANDED 2026-07-28 (merge 10053236): parse_dl.pl DCG + print_dl.pl + dl_view/ (109 fixtures as .dl text) + SYNTAX.md; round-trip 109/109, ghcacher.dl/conformance.dl parse with named gaps only. DCG is the CANONICAL parser (langium demoted). NOT yet wired into compile_fixture (term form still the compiler entry; wiring queued behind the latest/combine/zip spelling words). Hosts half of phase D still unbuilt.
```

## tsv2_unmarked_trigger

Source: `v6/prolog/ARCH.pl:707`

```text
LANDED same merge: any-body-atom occurrence lowering grounded in engine.pl trigger_items/occurrence_trigger; scoreboard 27 identical / 79 unsupported; three named unsupported constructs remain (edge_trigger_is_derived needs a tickLoop carry seam, edge_head_column_type_mismatch, edge_head_conflict_risk)
```

## clock_bind

Source: `v6/prolog/ARCH.pl:708`

```text
LANDED 2026-07-28 (merge 378a39cf): BindDef/BindRunner input twin of HostDef in 1_binds.ts, clock_period rows -> interval per period, bucket = floor(epoch/period), teardown rides program-swap switchMap; dl 90/90, endurance PASS. Known limits in 1_binds.ts header: config read once at subscribe (mid-run clock_period row needs reload); no input-side dedupe cache (wall-clock cadence has no witness -- real asymmetry vs effect_cache)
```

## phase5_ingest_binary

Source: `v6/prolog/ARCH.pl:721`

```text
LANDED 2026-07-30. v6/dl ingest resolves executable DL_EXTRACT_BIN override, then in-tree release extract, then one shared cargo release build; removes absolute debug-worktree default. Typecheck PASS, focused ingest 10/10, full DL 96 PASS/1 explicit skip. Release bad.ts census updated from stale 79 to 82 records. No language syntax/type/host change.
```

## extraction_live_p2

Source: `v6/prolog/ARCH.pl:722`

```text
LANDED 2026-07-29 night (opus worktree, 6 commits, coordinator re-ran everything in worktree AND merged main): watch bind on node fs.watch behind IWatchSource seam (zero deps; events collapse to watch(glob,path,digest) rows w/ arrival SIGN -- rename = -old/+new same batch, atomic save = digest change, identical bytes = zero delta; bufferTime(100ms) never debounce); enumerate/enumerate_at = git ls-files pathspec (tracked-only, node_modules never walked; ls-tree takes NO glob, oids via rev-parse); extraction host = generic sh host, demand (path,digest) content-addressed, in-tree RELEASE extract bin (cargo --features cli REQUIRED). LIVE DEFECT FIXED: __host_response_* keyed on witness digest alone lost all but last row of multi-row answers -- ordinal:int column, key (witness,ordinal), fail-first receipt, oracle+emitter same-arc. EXIT RECEIPT extraction-live.sh 8 phases HOLDS incl kill -9 exactly-once. STANDING: sg_pattern unsupported construct untouched; queryPlans emitted-not-executed; watcher restart+delete gap CLOSED 2026-07-29 morning (sol lane, merge ea938d6c: boot reconcile = engine rows vs git ls-files at subscribe, one boot batch, lastDigest seeded; A12 one-shot crossing sanctioned + recorded in the 2_binds.ts header); one-host-decl = one record shape (two shapes of same file spawn twice; fix named = (path,digest,family) content cache)
```

## memory_soak

Source: `v6/prolog/ARCH.pl:723`

```text
LANDED 2026-07-29 night (sonnet worktree, coordinator re-verified everything incl sabotage red): GET /stats (ServeStats: IServeStats; PRAGMA page_count/page_size/freelist_count + ONE grouped dbstat statement via json_each bind, forkJoin on the existing seam; dbstat PROVEN available on @libsql 0.17.4) + memory-soak.{sh,ts} (keyed-replace + log-keep(count) + derived edge churn, 2500 ticks; rss/heap/page-count/stmts-per-tick flat asserted on quarter means, sabotage keep_all goes RED exit 1). STEP-0 FINDING: rust has NO sqlite3_status wrappers -- v5's whole surface is db.rs rel_stats dbstat sums + health.rs PRAGMAs; tsv2 mirrors exactly that. justfile memory-soak recipe wired by coordinator, added to green-all. Banked finding: tests/serveHelpers startServed retains all events by design (fixture replay), a false-positive growth source at soak scale -- soak uses a private non-retaining subscribe (run-fixture precedent, outside the one-subscribe scan)
```

## prolog_org_refactor

Source: `v6/prolog/ARCH.pl:724`

```text
LANDED 2026-07-29 night (opus worktree, 12 commits, ALL 10 review ranks, coordinator re-ran everything in worktree AND merged main): prolog-lint gate ratcheted baseline 1 (in `just green` now, coordinator wiring); emit_ts collision renamed; 0_body_walk.pl walk_body/3 (10 sites); 0_program_check.pl (6 mirrored checks one impl + BOTH engine-only holes closed compiler-side w/ fail-first receipts); 1_expansion.pl declared phase order + enum context (analyzer double-expansion gone, spread phases are placeholder rows); expression operator inventory; R7 14 dead exports removed of 44 classified; R8 private sites 10 -> 1; R4 oracle aggregate classification on registry axis (oracle stays wider). Battery: conformance 137/0, plunit 124/124, TEXT_DOOR 72/72/0, sweep 72/70/0-wrong both modes, green exit 0. DELIBERATE moves: compiled+TEXT_DOOR 73 -> 72 (log_without_retention emitted a module the oracle rejects -- checked-in gen module DELETED), conformance +aggregate_in_edge_head_rejected. Journal: plans/2026-07-29-prolog-org-refactor-journal.md
```

## org_banked_findings

Source: `v6/prolog/ARCH.pl:725`

```text
4 findings banked in the org journal, each PINNED BY A TEST so drift is loud: (1) trigger_items/body_atoms misclassify next/combine/comparisons/lifecycle wrappers as relation atoms; (2) goal_rel_refs reports next/1+combine/2 as positive refs; (3) finalize_in_level_rule diagnostic drift + both doors accept not(finalize(...)); (4) 3 private cross-module calls in sprefa-store/bench/v1-scale-gen.pl outside the lint gate's load set. Fix wave = one small lane, unowned
```

## watcher_buy_research

Source: `v6/prolog/ARCH.pl:726`

```text
LANDED 2026-07-29 night (merge 8b0b49a8): plans/2026-07-29-watcher-buy-research.md. VERDICT @parcel/watcher first (native batch callback matches engine.submit(IArrivalBatch) one-tick commits, ignore-filter below JS, prebuilds all platforms, MIT, 31M weekly dl); node fs.watch = zero-dep fallback (IS chokidar v4/v5's mac/win backend); watchman = optional backend upgrade later, not the buy. Open residuals in doc: node ignore walk-vs-receive time on linux, watchman atomic-save, parcel symlink default. Pick gets its fork/5 row at phase-2 dispatch
```

## save_session_pl

Source: `v6/prolog/ARCH.pl:727`

```text
EXECUTED at the 20260729.0 save: chat_log/20260729.0.*.pl = module session_20260729_0 with session/2, in_flight/4, landed/2, session_task/2, awaiting_user/2, ruled_this_session/2; consult-verified. Convention: every save now emits the .pl sibling; load-session can consult it
```

## edge_body_constructs

Source: `v6/prolog/ARCH.pl:728`

```text
LANDED 2026-07-29 night (opus worktree, 4 commits, coordinator re-verified; sweep 72/70 -> 82/80 identical, 0 wrong, TEXT_DOOR 82/82/0, plunit 134/134): negation/comparisons/binds guard seam + now/1 emitted tick counter + edge-head column typing from feeding bodies. Refusals removed: edge_body_needs_{negation,bind,comparison,now} + edge_head_column_type_mismatch; added: edge_body_with_negation (not/1 beyond one plain atom), edge_body_with_now, now_in_level_rule (compiler-only, oracle solves it), edge_body_joins_arrival_fed_level (SEE tick_phase_alignment). Fallout fixes: analyze seeded_refs Initial-only refs silently dropped final-state rows; print_dl type synthesis keyed off missing col_type (48 dl_view regen). Receipts in SCOREBOARD.md
```

## tick_phase_alignment

Source: `v6/prolog/ARCH.pl:729`

```text
LANDED 2026-07-29 night (opus worktree, 2 commits, coordinator re-verified worktree AND merged main; sweep 82/80 -> 85/83/0-wrong both modes, TEXT_DOOR 85/85/0, plunit 137/137): (a) mid-tick level plane frozen where engine.pl freezes it -- recomputeLevelsBeforeEdges shares the emitter's 5 supportSql, naive recomputes before AND after edges; edge_body_joins_arrival_fed_level REMOVED, clock_rel_join_storms byte-identical (was 3-vs-1). THE FLAGSHIP SHAPE COMPILES. (b) separate __departure_frontier_<rel> TEMP table per listened rel (sign-column alternative rejected: touches every rel's DDL; receipt = all 83 prior modules byte-identical); departures in carryPending; finalize-in-edge flipped. HOLE FORCED SHUT en route: flipping the finalize registry row deleted the generic refused-goal catch, compiler ACCEPTED finalize-in-level -- finalize_in_level_rule restored in analyze shared_unsupported order, drift became an agreement test
```

## mutual_recursion_in_tick

Source: `v6/prolog/ARCH.pl:731`

```text
LANDED 2026-08-15: strat.pl:cyclic_head_groups/2 pairs every head on a positive INDIRECT stratum cycle with its group index (self edge still dropped, so the expand wavefront keeps direct self-recursion); both emitters render it as a `recursion_group` {group, round_cap, heads} on the level statement, absent on an acyclic head so every acyclic module's text is BYTE-IDENTICAL; both runtimes replace the single statement pass with sequence_level_rounds over maximal same-group runs, repeating until no statement moves a row (arrivals + collected zeroes, so a retraction-only round still runs again) and charging fixpoint_round_cap/1, which throws diverging_measure_recursion([path,reach], 1000) identically on both doors. Second defect the arc surfaced and fixed: topo_order_group/2's cycle fallback was PROGRAM order, splitting a multi-clause head around its cycle peer, and group_adjacent_by_head/2 folds only ADJACENT rules -- the emitted module re-issued the head's __support_next DDL and crashed. Fallback now orders by HEAD. Gates: conformance 448/0, sweep RUN 341 identical=335 wrong=0, grade byte-clean 334/448, plunit 5 known-red, golden-flex + typegen golden HOLD, typecheck 0. Fail-first: fixtures/24_mutual_recursion.pl both WRONG pre-fix (path 3 rows of 6; `table "__support_next_list_type" already exists`); tsv2 tests/mutualRecursionRounds.test.ts pins 20 statements per round and 61 per tick unchanged for an acyclic program. Vacuous gates corrected: incremental_program_safe/4 no longer walks a list to say true, retraction_guard/2 now reads recursive_level_refs/2 so a mutual cycle stops claiming plain-count-acyclic. PROBED 2026-08-14 (issues/mutual-recursion-permanently, repro in probes/): direct self-recursion settles tick 1 on all three doors; INDIRECT (mutual) recursion under-derives PERMANENTLY on both emitted doors (3/6 closure rows) while oracle computes the least fixpoint tick 1 -- TS naive mode on the same emitted module is correct, so engine defect not semantics. Sites: rules_read_head_recursively/2 direct-only (lower.pl:4445,5179-5186 -> ExpandPlan=none for cycles), single-pass statement loops (1_incremental.ts:1120-1131, incremental.rs:975-986), vacuous incremental_program_safe (emit_ts.pl:2624-2631), hardcoded incremental_safe(true) w/ no Rust naive fallback (emit_rust.pl:63), Kahn fallback Ordered=Group (strat.pl:96-101) contradicting the emit_ts.pl:2054 comment. Direct spelling refused loudly (built_text_in_recursive_head lower.pl:5249); two-rel spelling silently wrong. Corpus blind spot: engine_core.pl:452-462 even/odd is depth-1, one pass closes. DECIDED 2026-08-15 (delegated provisional, alternatives noted not rejected, issues/mutual-recursion-permanently decision note): close in-tick with outer rounds over the stratum statement pass until quiescent, capped by divergence_backstop; alternatives kept open: refuse-by-name (blocks dl6-first typegen), naive route (no Rust naive door)
```

## divergence_backstop

Source: `v6/prolog/ARCH.pl:732`

```text
LANDED 2026-08-15 PR #263: fixpoint_round_cap/1=1000 at lower.pl:4543 carried in the expand plan; both doors abort the tick byte-identically with diverging_measure_recursion(Rel, 1000); Rust door's placeholder recursive-CTE path replaced by a port of the TS wavefront -- ONE shared recursion driver, graded.tsv +2/-0. Fail-first receipt: counter+1 program compiled rc=0 and hung all three doors (rc=124 at 30/45/45s); post-fix aborts 0.696s. Fixtures 23_diverging_recursion.pl (throw + bounded control); noted user-visible semantic: 1000-round bound, one line to change
```

## gen_staleness_gate

Source: `v6/prolog/ARCH.pl:733`

```text
LANDED 2026-07-29 morning (sonnet worktree lane, coordinator review-merged): v6/tools/staleness-gate.sh in green-all -- gen half enumerates gen_emitted modules absent from manifest compiled set, regenerates via compile_dl6.sh + diffs (door-handwritten covered, unknown provenance = named FAIL); binary half fails when src .rs/Cargo.toml newer than existing target/release/{dl,extract} (missing binary = pass, receipt scripts own building). Sabotage receipts in script header. FIRST MAIN-TREE RUN CAUGHT A REAL ONE: extract binary predated terra's --resolve types.rs merge, gate red -> rebuild -> green. Agent also found+fixed a pipefail SIGPIPE race in its own draft (grep -q closing early)
```

## flagship_callgraph

Source: `v6/prolog/ARCH.pl:734`

```text
LANDED 2026-07-29 night (opus worktree, 2 commits, coordinator re-ran the rig + full battery): examples/callgraph-ast.dl BYTE-UNMODIFIED vs its v6 port over a pinned 13-file rust corpus; flagship-callgraph.sh + flagship-classify.py; every diff row bucketed, 0 expression gaps, 0 defects (def +10 = function_signature_item, call +181 = method/path/struct-literal sites v5's bare-identifier ast query cannot match; calls/unused proven by RULE FIDELITY -- v5's rule bodies run against each engine's own inputs); unused inverts as the anti-monotone rel must. 2 fixtures promoted (conformance 139/0, sweep 87/85/0, TEXT_DOOR 87/87/0). In green-all as `just flagship`
```

## flow_interproc_port

Source: `v6/prolog/ARCH.pl:735`

```text
UNBLOCKED then PORTED 2026-07-29: portable callable-plane port merged d322c93f (terra) with every gap named, value-plane rewrite merged 837fe7f2, rig grading ed81cdc6. The program is v6/dl/fixtures/flagship-flow.dl6 and it runs against v5's own std/flow.dl output on the pinned corpus (`bash v6/tsv2/scripts/flagship-flow.sh`). PORTED IS NOT PARITY -- see flow_parity_residue for the three open columns. closure/reaches over the extraction feed is graded here as flow_reach (9112 matched); the general closure() spelling still rides the graph-algo queue item
```

## extract_resolve_flag

Source: `v6/prolog/ARCH.pl:736`

```text
LANDED 2026-07-29 morning (merge 17778bbb + 0_prolog ledger refresh c26b4e0e, codex terra, worktree+branch removed); the staleness gate then caught the stale extract binary twice on the same day, which is how the merge was proven live. Original brief text follows. USER-WAIVED extractor touch, smallest correct: wire the EXISTING library-tested Resolve pass (tests/0_prolog.rs:70-95 is the exact recipe -- def index over all files, ProjectCx, per-file resolve) to a project-mode CLI entry emitting FLAT resolved-edge JSONL + a CLI-level golden test pinning the phase-2 contract that never existed (bin was phase-1 BY DESIGN, extract.rs:176; lib tests covered resolve; nothing asserted bin-vs-lib capability parity -- that asymmetry is the lesson). Brief plans/2026-07-29-extract-resolve-flag-brief.md. DISPATCHED codex terra, no-commit flow
```

## extra_drain_tick

Source: `v6/prolog/ARCH.pl:737`

```text
FIXED 2026-07-29 morning (coordinator, main tree): recomputeLevelsAfterEdges refCount branch staged reconcile re-INSERTs into nextFrontier phase 1 (the P3 shape its own aggregate-branch comment had already called out as the unfixed asymmetry) -> promoteFrontiers read them as carry -> one empty drain. Fix = frontier copies [] on the reconcile call, matching the aggregate branch's afterEdges=false rationale (reconcile = same-closure correction, never post-write growth). Fail-first receipt in tests/extraDrainTick.test.ts: truncated 4-tick callgraph_unused schedule -> 5 lines w/ {"tick":5,"deltas":{}} red, 4 green; full-schedule oracle byte-identity guarded in the same file. Sweep both modes 87/85/0-wrong zero movement, tsv2 58/0/1skip
```

## cq_bundle_lane

Source: `v6/prolog/ARCH.pl:738`

```text
LANDED (merge 733d4c1b, coordinator re-ran conformance 158/plunit 138 in-worktree AND full battery on merged main: sweep both modes 98/96/0-wrong, TEXT_DOOR 98/98/0, roundtrip, staleness gate, LSP DIAGS HOLDS w/ 0+0 workaround REMOVED): groupby literals emit (N+0) (8 diag modules moved as predicted); probe guards fixed by GOAL PLACEMENT in host expansion (pre-probe goals -> demand rule, post-probe guards after response -- root cause, not a bound-set widen); 4 drift pins flipped; 0_unsupported_messages.pl ONE umbrella prolog:message//1 over 77 dynamically inventoried unsupported construct signatures + coverage test. Residue: locations say rule-index unavailable (parse_dl keeps no source positions). Was: codex sol, base 67719352, ../sprefa-codex-cqbundle: carries emitter_groupby_literal + probe_output_guard + org_banked_findings + b4_unsupported_messages (prolog:message//1 umbrella w/ coverage test) IN ORDER; brief plans/2026-07-29-compiler-quality-bundle-brief.md; phase 5 + struct_host_output_seam both QUEUE BEHIND this lane (shared compile files)
```

## crawl_bench

Source: `v6/prolog/ARCH.pl:739`

```text
LANDED 2026-07-29 (merge a192cd35, codex luna, review-gated; coordinator re-ran the bench itself and matched luna's stmts/tick 54.03 exactly). v6/tsv2/CRAWL-BENCH.md + scripts/crawl-bench.sh, hermetic scratch, ~/orgs/grafana read-only, nice -19 (the managed host REFUSED the niceness and said so), NOT in green-all. THE NUMBER: v5 org-fan 42,739 files / 389 repos / 12.07s = 3,540.9 files/s; v6 served extraction 779 files / 8 repos / 19.15s = 40.7 files/s. That is ~87x on the same machine, and the v5 memory-doc 7,244 files/s (5.9s) is a SECOND yardstick this run did not reproduce -- the honest v5 number on this host today is 3,541. Stated gaps in the doc: v5 reads a git tree at HEAD, v6 hashes the working tree; v6 has NO org fan-out spelling at all (the shell loop supplies it, one served process per repo); v6 runs cst+type+call+df families where v5 does a scan fact. 250 of 389 repos are usable for v6 (139 have no go/ts/tsx); the default --max-repos 8 exists because the linear projection of the full corpus is 1,050s
```

## flow_parity_upgrade

Source: `v6/prolog/ARCH.pl:740`

```text
MERGED 2026-07-29 (837fe7f2, terra's ccfe53ec) once the seam opened, then GRADED the same morning (ed81cdc6): df hosts + arg->param positional hop + sig-owner joins + 2 fixtures (5_flow_value_plane.pl). FOUR-QUERY TABLE, first real-seam contact: flow_edge v5 2462 / v6 2184 / matched 2184 (v6 is a strict subset, 278 v5-only); flow_reach 9112 matched + 177 v6-only reflexive rows; flow_param_type 0 matched; flow_node_type EMPTY v6-side. Three fixes the contact forced: `= concat` -> `:=` at 7 sites (`=` refuses under the WRONG name, filed as SLOT-BIND-SPELLING), smaller_type_owner was missing its df_value join (the compiler unsupported construct was CORRECT), and the referee learned to translate coordinates (fork rig_coordinate_translation). Residue = task flow_parity_residue
```

## struct_host_output_seam

Source: `v6/prolog/ARCH.pl:742`

```text
LANDED 2026-07-29 (merge 265da55f, codex sol, review-gated; in-worktree battery conformance 160/0, plunit 140/140, sweep both modes 99/97/0-wrong, TEXT_DOOR 99/99/0, tsv2 69/1skip; merged-tree sweep 100/98/0): decl-B host OUTPUT columns admit declared type names via host_output_columns (WRAPPER spellings keep the column_type_wrapper unsupported construct, unknown names stop as column_type_unknown); serve carries host stdout across the arrival seam as JSON text and StructPlane decodes text for ref columns before shape-check + intern; IHostColumnPlan.type widens to string and is refused pre-emission by the shared type-plane check. Fail-first receipts both directions in 4_struct_values.pl. UNBLOCKED TWO ARCS: flow_parity_upgrade merged immediately after, and the comment lab's technique-1 payload destructure (which hit the same wall independently)
```

## probe_output_guard

Source: `v6/prolog/ARCH.pl:743`

```text
FIXED by the cq bundle (merge 733d4c1b) and the fix was NOT the one the row predicted: the diagnosis said "widen the bound set", the root cause was GOAL PLACEMENT in host expansion (pre-probe goals belong in the demand rule, post-probe guards after the response), so the widen would have papered it. Fixture probe_output_comparison_guard (5_compiler_quality.pl). Original symptom: a comparison guard over a probe OUTPUT var in a level rule refused as unsupported_construct(unbound_head_var(_G)) -- wrong name, no location, review-B4 in the wild. RESIDUE: locations still print "rule-index unavailable" because parse_dl keeps no source positions
```

## cli_bop

Source: `v6/prolog/ARCH.pl:744`

```text
LANDED 2026-07-29 night (sonnet worktree; agent left tree UNCOMMITTED-but-clean, coordinator reviewed file-by-file, ran every receipt itself, committed on the branch -- process deviation logged): registry cli_command/3 inventory -> commander bop.ts, verbs serve/run/check/load/q, run+check boot serveTsv2 IN-PROCESS (no daemon), exit contract 0 clean/2 named-unsupported construct/1 broken verified by coordinator runs (ghcacher = 2 w/ recursive_stratum receipt). 12 tests + inventory-parity (swipl vs commander lines); one dep commander (user-required); one-subscribe 1/1 both apps (cli/ = run-fixture exemption). halt-inside-catch swipl trap documented in bop_check.pl. THE 6.2.0 TAG GATE IS SATISFIED; push = user. LSP milestone still open on task 11
```

## prolog_folder_flatten

Source: `v6/prolog/ARCH.pl:745`

```text
2026-07-31 luna lane, user-approved verdict-4c repair: 9 files (3_clock_check/6_profile/analyze/compile/emit_ts/lower/print_dl/strat/sweep) moved compile/ -> v6/prolog, compile/ = base layer only (registry/parse_dl/oracle_dump/emit docs), production upward references 0, 69 reference sites updated, full battery identical pre/post. The folder cycle behind all 6 hand-numbering errors is dead; prolog folder ordering now derivable.
```

## battery_load_flake

Source: `v6/prolog/ARCH.pl:746`

```text
CLOSED 2026-07-31 same day: root cause = spawn-heavy tsv2 tests (bop run boots node+swipl per test) x node --test default file concurrency = ncpu (12 here); bop quiesce reliably crossed its 30s timeout in-battery post-flatten while passing isolated at 1s, serial suite 127/0. Fix: --test-concurrency=6 in tsv2 package.json test script; suite 127/0 in 4.9s, green exit 0. The test header's own 10s->30s timeout chase (2026-07-30) was this same class; the bound fixes the class, not the test.
```

## golden_flake_hunt

Source: `v6/prolog/ARCH.pl:747`

```text
RCA LANE RAN 2026-07-29 morning (sonnet worktree, diagnostics merged): REPRODUCED 1/18 sub-runs under 3x-concurrent full-suite load -- all 11 subtests print pass yet the FILE fails with bare 'test failed', ZERO error payload (native/process-level signature, not a JS assertion). Ruled out: tmp paths/ports (all :memory:, zero fs), memcap singleton (process isolation confirmed), RSS budget (peak 186MB vs 512MB, rss subtest passed in the failing run). Leading candidate unproven: ~40 unclosed :memory: @libsql clients per run. Landed: additive exit-listener diagnostics (pid/exitCode/rss/storesOpened printed on nonzero exit only) so the next occurrence carries context. Rate too low (5.5%) to validate any fix inside a lane
```

## reactor_buffertime_flake

Source: `v6/prolog/ARCH.pl:748`

```text
NEW, found by the golden RCA lane: v6/sprefa-store/js/tests/labs/reactor.test.ts "reactor A file+folder coalesce" is a MORE frequent real flake (6/18 under 3x load; AssertionError actual [1] vs expected [1,2,3]) -- wall-clock bufferTime coalescing assertion, the same class F3 killed in v6/dl with TestScheduler/virtual time. Fix shape known; small lane, unowned
```

## lsp_diags

Source: `v6/prolog/ARCH.pl:749`

```text
LANDED 2026-07-29 night (sonnet worktree after 2 coordinator continue-nudges, committed 77b5bbce, coordinator re-ran receipt: LSP DIAGS HOLDS): ZERO new LSP code -- diag-rail.dl6 declares rel diag_v5 in v5's exact 9-column shape and tsv2's bare-name tables (lower.pl table_name) make it THE table src/lsp.rs:545 selects; bridge fully in-language. Real v5 dl --lsp --diag-db over real stdio JSON-RPC: publishDiagnostics appear + retract for no-eval + unused-def rails over the live watcher/extraction feed. Line numbers HONESTLY 0 until decode_arc lands spans. Sabotage: column rename passed engine-side (positional curl) and went red at the real LSP client. DELETED from green-all and from the tree: the whole receipt is a `dl --lsp --diag-db` subprocess and Chris's "I DO NOT WANT TO RUN V5 ANYTHING ANYMORE" leaves no arm for it; the v6-native replacement is plans/2026-08-12-v6-native-lsp.PLAN.md
```

## emitter_groupby_literal

Source: `v6/prolog/ARCH.pl:750`

```text
REAL EMITTER DEFECT (lsp arc), FIXED IN TWO PASSES: a rule head with >=2 bare integer-literal columns reaches the GROUP BY verbatim and SQLite reads a bare integer there as a POSITIONAL column ref -> SQLITE_ERROR "Nth GROUP BY term out of range". Pass 1 (cq bundle, merge 733d4c1b) wrapped literals as (N+0) on support_group_exprs and REMOVED the 0+0 workaround from diag-rail.dl6; 8 diag modules moved as predicted. Pass 2 (coordinator, 6522f848, from the altitude review's finding 1) found aggregate_group_exprs -- the scoped-delta insert + recompute path -- still emitting bare integers, and gave both sites ONE shared group_expr/3. Fail-first fixtures: groupby_two_bare_integer_literals and groupby_aggregate_two_bare_integer_literals (5_compiler_quality.pl), each RED with the SQLITE_ERROR and GREEN oracle-identical both modes. THE LESSON WORTH KEEPING: the first fix was verified by a fixture that only exercised one of the two call sites
```

## v5_lsp_exit_hang

Source: `v6/prolog/ARCH.pl:751`

```text
FIXED 2026-08-02 (opus lane t5-lsphang, merge 721be80a on codex/rel-ref-file-span-lab, unpushed): root cause = background sender clones, not the loops -- the daemon-push subscriber (lsp.rs:200) and the --diag-db poll thread each hold a connection.sender clone for their whole life and neither is interruptible; lsp-server's writer thread ends only when every clone drops and IoThreads::join waits on the writer, so join blocked forever (stack sample: main in pthread_join, LspServerWriter parked in recv). Fix = finish_lsp drops the transport instead of joining + exit-code contract (shutdown-then-exit = 0, anything else incl bare exit / stdin EOF = 1); 3 regression tests tests/it/lsp_exit.rs. lsp-diags.sh belt-and-suspenders pkill now moot on fixed binaries. WAS: v5 DEFECT disclosed by the lsp receipt, reproduced standalone: dl --lsp --diag-db answers shutdown correctly then hangs after exit + stdin EOF; receipt SIGKILLs after both directions proven. Owner src/lsp.rs, v5 side. ESCALATED 2026-07-29 morning: the receipt's SIGKILL does NOT reach every path -- coordinator found 3 hung dl --lsp processes ~4h old (one per lsp-diags run) and killed them; lsp-diags now rides green-all so EVERY battery run can leak one. Until the v5 fix, lsp-diags.sh owes a belt-and-suspenders pkill of its own spawned pid on exit. MECHANISM CONFIRMED by the audit 2026-07-29: the script's stop_all trap kills DRIVER_PID, but DRIVER_PID is the PYTHON driver (lsp-diags.sh:239, scripts/lsp_diag_driver.py) and `dl --lsp` is its CHILD -- kill -9 on the parent orphans the binary that is already refusing to exit. Still unfixed at b535ca62 (zero pkill in the script), and lsp-diags is in green-all, so every full battery run can still leave one
```

## pre_occurrence_loop

Source: `v6/prolog/ARCH.pl:752`

```text
LANDED 2026-07-30. Ordered occurrence execution snapshots each referenced rel once into __pre_<rel>, processes arrivals/departures in sequence, applies each accepted keyed write before the next occurrence, recomputes levels, and stages carry through existing frontiers. No surface or type construct. 13 former edge_body_needs_pre fixtures compile; focused runtime 2/2 PASS; PLUnit pins one snapshot plus mirrored writes. The earlier higher_order_scan gap receipt was false-positive because catch/3 left its error variable unbound on success.
```

## struct_as_rows

Source: `v6/prolog/ARCH.pl:754`

```text
LANDED 2026-07-29 morning (opus worktree, 6 commits, merge dcfa6fcc; coordinator resolved manifest/run-results by regen, re-ran EVERYTHING on merged main: conformance 156/0, sweep BOTH modes 95/93/0-wrong, TEXT_DOOR 95/95/0, plunit 137/137, roundtrip ALL PASS, tsv2 65/1skip, dl 96/1skip, store 74/74, staleness gate OK, 0_prolog ledger 54->56): type name(col: type) decl (SQL word, NOT rel -- edge 2 forced it: a rel-spelled struct makes its dictionary nameable), intern-at-arrival dictionaries w/ __semantic full canonical text (no UDF hash exists) + __rendered memoized JSON, boundary render join (EXPLAIN: rowid SEARCH), decode/2 = sugar lowering to dictionary join via rule rewrite across all 7 level families, spans = declared struct (host columns accept type names). BOTH EDGES GRADED: values-never-ids (intern_order_a/b different dense ids, identical bytes both orders) + dictionary boundary-invisible (structural: dict relplans reach body compiler only). Migration receipt STRONGER than header: inline twin emitted log NEVER matched oracle (obj-term text vs canonical JSON) -- ref side byte-identical, migration is the fix. +16 fixtures (7 identical/9 named unsupported constructs); 2 json fixtures -> sharper decode_source_not_struct; 9 temporal_pipe compound-destructure stay (SLOT-TERM-STRUCT: prolog compound is NOT a struct spelling). Defect fixed en route: sweep.ts final-state encoder had drifted from ticklog canonicalization -- one TickLogEmitter.valueText now. Slots for user: SLOT-ARRIVAL-CANONICAL-ORDER (oracle refuses non-sorted keys; lifting = absorb_arrivals canonicalization ruling), SLOT-GC-TIMING debt + host-output seam = rows below
```

## key_position_validation

Source: `v6/prolog/ARCH.pl:770`

```text
LANDED IN ACTIVE BRANCH 2026-07-29. Existing key(...) now rejects zero and above-arity positions as key_position_out_of_range(Ref,Position,Arity), and repeated positions as key_position_duplicate(Ref,Position), before DDL. Shared 0_program_check invariant, identical oracle/compiler unsupported construct terms, 5 focused plunit receipts. Battery: plunit 147 PASS, conformance 163 PASS, key census 12 PASS. COST: no parser/SQL/runtime/surface change
```

## keyed_signed_row_clock

Source: `v6/prolog/ARCH.pl:771`

```text
LANDED IN ACTIVE BRANCH 2026-07-29, scopes.pl stale_keyed_retraction_keeps_replacement, oracle 3 expectations PASS and emitted SQLite 3 PASS. LOCKED CLOCK: +row may replace the current row sharing its key and reports -old/+new in that tick; -row retracts the exact row named, so delayed -old after +new is silent and cannot delete the replacement; -current removes it. This follows the existing signed-full-row event shape and adds no key-delete operation
```

## automatic_derived_reference_match

Source: `v6/prolog/ARCH.pl:775`

```text
LANDED ACTIVE BRANCH 2026-07-29. New shared expansion phase 50, after match: relation-shaped level-head values add ordinary target membership; edge-head values add latest(target) to sample without another trigger. Dependency is visible to oracle, checker, stratifier, SQL planner, and emitter. Missing target yields no parent row. Clock receipt: keyed target edge creation and parent level membership settle same tick; target replacement emits -old-parent/+new-parent same tick. COST: no surface/runtime primitive. Battery: expansion plunit 150 PASS, conformance 164 PASS, fixpoint clock 6 PASS, construction contexts 8 PASS. NEXT world/boot key-driven batched resolution
```

## world_reference_key_resolution

Source: `v6/prolog/ARCH.pl:777`

```text
LANDED 2026-07-30. One rel/checker/table graph: nested rel-shaped ingress expands topologically into ordinary public target arrivals, then rewrites parent columns to dense integer endpoints. Target rows, parents, and target-dependent level rows publish inside one external tick through the mode's ordinary arrival path. StructPlane.intern returns IArrivalBatch; no special result type, ref construct, dictionary, stored JSON, or new syntax. Key conflict and stable-id rules remain. Gates: PLUnit 167/167, conformance 165/165, typecheck PASS, struct runtime 11/11 incremental + 11/11 naive, both sweeps 115 total/113 identical/0 wrong/2 known run errors.
```

## ref_necessity_proof

Source: `v6/prolog/ARCH.pl:778`

```text
LANDED ACTIVE BRANCH 2026-07-29, v6/prolog/labs/rel_value_unification/11_ref_necessity.pl, 7 PASS. No ref surface construct selected. Existing mechanics cover the cases: target scan binds dense __id; typed variable forwards it with no target rejoin; decode joins __ref_<target> when fields are needed; arbitrary graph cycles are rows in a separate edge rel with two entity endpoints. DEFAULT-PATH DEFECT FIXED: full recompute projected __id while incremental frontier emitted JSON; frontier now rejoins the current target row and projects __id. INLINE recursive entity columns remain type_cycle: relaxing them produced recursive render views and no finite full-value boundary representation. KEYED TARGET ID FIXED: arrivals use ON CONFLICT DO UPDATE instead of INSERT OR REPLACE, preserving __id and every stored parent endpoint across non-key replacement. Runtime receipt 11 PASS, plunit 150 PASS, sweep 101 identical/0 wrong/2 recorded errors
```

## reference_membership_boundary

Source: `v6/prolog/ARCH.pl:779`

```text
IMPLEMENTED 2026-07-30 using the already locked one-rel normalization model. Nested wire values become same-tick ordinary target arrivals before parent reference rows. New relation_reference_target_and_parent_share_tick fixture proves target, parent, and target-dependent level addition at tick 1. The former delta-silent membership exception and identity-catalog split are closed.
```

## fork_join_malformed_json

Source: `v6/prolog/ARCH.pl:783`

```text
CLOSED 2026-07-31 (opus worktree, brief plans/2026-07-31-forkjoin-defect-brief.md). RCA: the failing statement is any_failed's incremental level insert, INSERT OR IGNORE INTO "any_failed" ("status") SELECT DISTINCT json_extract(d0."col1",'$.args[0]') FROM "__frontier_outcome_a" d0 WHERE d0."_phase" >= 0 AND json_extract(d0."col1",'$.fn') = 'error'. Two encodings for one compound value: an ARRIVAL writes canonical term text (sweep.pl term_text/2, ok(body_one)) and a HEAD EXPRESSION writes the json1 tagged form (lower.pl compile_expr, {"fn":"ok","args":["body_one"]}); compile_pattern_arg/7 destructures the second, and json_extract over non-JSON text is an sqlite ERROR not NULL (measured). OUTCOME = NAMED REFUSAL compound_pattern_on_arrival_rel(Ref, Position, Pattern) in analyze.pl, compiler-only in the now_in_level_rule slot -- the oracle keeps executing the program and conformance is untouched. A real fix was priced and rejected on the brief's own condition: aligning the arrival encoding pre-empts ruling compound_storage = struct_as_rows, AND still grades WRONG, because compile_sub_args/7 types a destructured sub-argument `text` by the inline-flat compound punt, so error(502) stores 502 as TEXT (measured) where the oracle prints the integer. Typing sub-arguments IS the struct plane; struct_as_rows deletes this unsupported construct. BUCKET SPLIT LANDED with it: sweep.ts run_error -> rejection (oracle throws too) vs emitted_crash (oracle completed, emitted module died), discriminated by whether out/<name>.oracle.jsonl exists, and the sweep now EXITS 1 on emitted_crash > 0 (sabotage receipt in the sweep.ts header). Battery: conformance 264 PASS unchanged, plunit 263 -> 267, TEXT_DOOR 190/190/0 -> 189/189/0, sweep 190 compiled/188 identical -> 189 compiled/188 identical/0 emitted_crash/1 rejection, both emitter modes, zero other fixtures moved. TWO RESIDUES, named in analyze.pl, both absent from the corpus (measured over all 265 fixtures) and both deleted by struct_as_rows: ONE HOP (an arrival compound copied whole into a derived rel and destructured THERE is uncovered; covering it needs the per-column encoding dataflow) and LEVEL ONLY (edge JOINED atoms reach the same lowering; the edge TRIGGER position is already refused, more precisely, as trigger_arg_not_var). The level-only scope was MEASURED not assumed: the first draft walked edge bodies and silently restated trigger_arg_not_var as this class on async_state_machine_with_pattern_scan + same_tick_error_then_fresh_chains_arms, rewriting their dl_view; caught by diffing the manifest against the base, now pinned by plunit edge_trigger_compound_keeps_its_own_unsupported
```

## match_left_to_right_surface

Source: `v6/prolog/ARCH.pl:788`

```text
LANDED 2026-07-30 from USER RULING 2026-07-29. Optional leading semicolon; Guard |-> Head is the existing level arm and Guard |+> Head the existing event arm. Parser/printer migration only; 0_match_expand erases both into ordinary Head <- Source,Guard or Head <+ Source,Guard rules. 167/167 PLUnit PASS. Roundtrip G2 real files parse with zero findings; G1 remains the known 147/164 with 17 removed-type not_variant fixtures.
```

## v6_2_host_contract_cleanup

Source: `v6/prolog/ARCH.pl:794`

```text
LANDED 2026-07-30. RHS host calls use ordinary relation spelling; RHS prefix/postfix question-mark and at-sign salt riders refuse. Registered exact positional input_roles preserve identity versus freshness inside existing probe IR. Checked host_plan/7 carries executor key; TS HostRunner dispatches shell or sprefa_extract. PLUnit 167/167 and conformance PASS; extraction clock 3 ticks PASS; ghcacher clock 5 ticks/final PASS; served-host focused 2/2. No new host syntax. Shell overuse remains measured in the scale-gate row.
```

## extraction_host_batching

Source: `v6/prolog/ARCH.pl:796`

```text
LANDED 2026-07-30. Compiler classifies fixed DL_EXTRACT_BIN templates as sprefa_extract; callgraph/diagnostics/flow projections share multi-family commands. HostRunner groups same-frontier extractor demands by executor, template, and ordered typed inputs; one stdout projects into ordinary response rels and witnesses settle independently. Shell remains singleton; cached sibling projections omit independently. Gates: PLUnit 168/168, batching lab 9/9, runtime 4/4, typecheck, extraction clock 3 ticks/final empty/59 response statements, flagship callgraph 13 files/0 unclassified. Served 7/8 with the remaining old fixture still spelling removed type and RHS question-mark syntax.
```

## rtkq_extraction_golden

Source: `v6/prolog/ARCH.pl:797`

```text
LANDED 2026-07-30. sprefa-extract adds typed AstPatternQuery/AstCaptureFact and CLI --ast-pattern/--ast-selector/--ast-capture; one parse batches four literal/contextual ast-grep patterns and emits flat query/capture/text/capture-span/match-span rows. DL6 unions create/inject and generic/plain rules, joins by whole-match span, and contains endpoints in API scopes. Golden: 5 ticks, demand +2/-2, 2 extractor processes, response rows 9 then 6, exact endpoint rows 3 then 2, final four rels empty after delete. Focused Rust 2/2, DL compile, and served golden PASS. No DL syntax/type/storage/compiler/runtime addition. Full extractor 16/18: stale shared corpus expected-count and sandbox-blocked scip-go proxy are named external/pre-existing failures.
```

## v6_2_scale_gates

Source: `v6/prolog/ARCH.pl:798`

```text
COMPLETED 2026-07-30. WATCHER: real WatchBindRunner/LiveEngine/file-SQLite receipt. 100/1000 files, 480/4800 events, 3 ticks, 1 subscription, 1.185185 write amplification, 90/900 exact final rows, 0 wrong; wall 39.198/265.241292ms, RSS 153845760/189202432B, SQLite 40960/253952B. GRAFANA: gate exit 0 over 389 discovered repos/250 usable. V5 full 42739 files, 389 repos, 13.00s, 3287.62 files/s, 356794368B RSS, 52387840B DB. V6 pinned first 8 usable, 779 files, 19.35s, 40.26 files/s, 176553984B RSS, 1069056B DB, 54.03 statements/tick. HOST OVERUSE: green V6 throughput exposes the per-witness extraction subprocess boundary; batching or V7 direct Rust link remains required.
```

## v6_2_http_cli_dogfood

Source: `v6/prolog/ARCH.pl:799`

```text
LANDED 2026-07-30. Canonical registry http_route/3 + cli_command/3 facts generate v6/tsv2/cli/0_inventory.ts through compile/2_emit_cli_inventory.pl; explicit handlers remain hand-written. bop load/run/q already use HTTP; stats now GETs /stats and ticks streams /ticks SSE. Focused HTTP dogfood 8/8 PASS. No DL syntax.
```

## norm_runtime_parity

Source: `v6/prolog/ARCH.pl:800`

```text
LANDED 2026-07-30. Existing norm/1 expression call only: registry text_scalar row, analyzer text operand rule, and emitted SQLite recursive scalar expression retain ASCII alphanumerics and lowercase letters. No parser, host, shell, or surface change. Receipts: full PLUnit 167/167; emitted runtime covers 'Route /V2: Café_42' -> routev2caf42, '---' -> empty, and 'AZ-09_é' -> az09; integer operand refuses by name.
```

## coordinator_pause_checkpoint

Source: `v6/prolog/ARCH.pl:803`

```text
PAUSED 2026-07-30 for coordination from another session. Every dirty tracked and untracked file was checkpointed together by explicit user instruction. Completed bool/float production work and unfinished clock-checker work coexist in the checkpoint. Resume from chat_log/20260730.0.v6-2-ts-closeout.pl. Clock boundary: historical A2 is not statically provable under current either-source trigger semantics; A6 needs inferred-offset versus runtime-tick comparison.
```

## single_rel_type_system_audit

Source: `v6/prolog/ARCH.pl:806`

```text
COMPLETED 2026-07-30. Production reference runtime uses ordinary target arrivals followed by integer-edge parent arrivals in one tick. The temporary IReferenceResolution wrapper was removed; StructPlane.intern returns ordinary IArrivalBatch. Scan/match and JSON labs are reconciled under the same model. Zero relation-like intermediate types, stored nested JSON/dictionaries, ref constructs, parallel checkers, or new syntax.
```

## assign_composition_lab

Source: `v6/prolog/ARCH.pl:815`

```text
LANDED 2026-07-29 (opus lane, merge d0104974): plans/2026-07-29-assign-composition-verdict.md. Census 30 real := sites: 14 map, 15 scan/pre folds, 1 naming-for-reuse, zero other. Whole-corpus desugar prototype graded 19/19 identical; paired compiler modules byte-identical. Seven concat-coordinate sites dissolve under file_span. Verdict: := is already sugar over argument-position expressions; a rel head write is next, := is a local name only. Three user cards remain: keep status quo vs shared expansion vs remove; expression evaluation in constructor/edge-head positions; fate of = and zero-use is/2. Two defects found: constructor sub-arguments diverge oracle vs emitter, and edge-head arithmetic unsupported construct is stale
```

## finish_the_job_epic

Source: `v6/prolog/ARCH.pl:816`

```text
LANDED 2026-07-29 (opus lane, merge 21ecd6ac): plans/2026-07-29-finish-the-job-epic.md supersedes v6-alpha-golden-plan; 12 epics, 12 user cards, codex-driveable owner map. Measured tail: 61 unsupported = 41 intentional named unsupported constructs + 26 construct debts (pre 13, JSON destructure 9, aggregate heads 4). Critical path E1 simplify -> E2 phase 5 bool/float/checker/ingest -> E7 schema import; E3 span/comment -> E4 flow residue -> E8 analysis exam runs alongside. Carries bool/precision rulings and the decl-legibility cluster; implementation remains represented by the individual ARCH task rows
```

## clock_checker_full

Source: `v6/prolog/ARCH.pl:819`

```text
LANDED 2026-07-30 (merge ffcddfc7): sol implemented, OPUS REVIEW GATE FALSIFIED the A6 "proven" claim by sabotage (constant observer + Grade=0 hardcode both stayed green), sol applied the 3-item fix list (nonzero-offset pipe crosscheck pinning a 4-tick delta sequence, clock_boundary negative, ClockCatch pinning restored, A6 renamed runtime_clock_crosscheck). Focused 24/24, plunit 197/197 coordinator-verified. The review-gate pipeline is the reusable lesson: the one affirmative claim was the one wrong claim
```

## roundtrip_two_door_fix

Source: `v6/prolog/ARCH.pl:820`

```text
LANDED 2026-07-30: the 18 roundtrip reds were ONE defect (parse_dl keeps both decl forms, print_dl printed both = duplicate decl line, reparse dropped the type). Fixed as the measured coupled pair: print_dl shadowed_by_type_decl/2 guard + explicit col_type rows in 4_struct_values.pl. roundtrip ALL GRADES, plunit 197/197, text door 123/123/0, conformance 175. Branch gate green again
```

## comment_rail_wiring

Source: `v6/prolog/ARCH.pl:821`

```text
LANDED 2026-07-30 (merge 786b5daa, luna no-commit flow, coordinator re-ran gate): 6 of 7 verdict techniques as standing rails + parity referee, comment_node 745/745 v5-exact. Named skips: markdown grammar (doc_format_extraction owns), block-pairing refused unbound_head_var (MISNAMED-unsupported construct suspect, equals_unsupported_by_name class)
```

## json_language_recovery

Source: `v6/prolog/ARCH.pl:822`

```text
LANDED 2026-07-30 (f9bf09df): plans/2026-07-30-json-query-language-recovery.md -- the v3/v4 json language is NOT lost, it is v5's src/datapath.rs brace walker (9 productions, stable since v1); v6 dropped it. 22 constructs: 8 express today / 4 ugly / 6 storage-carded / 5 new-surface ALL on the key axis. 9 user cards, top leverage CARD-KEY-CAPTURE
```

## filespan_reconcile

Source: `v6/prolog/ARCH.pl:823`

```text
LANDED 2026-07-30 (f94fc5c1): plans/2026-07-30-file-span-spine-reconciled.md -- 11 of 14 cards SETTLED by the v6.2 locks + landed rel-ref runtime, 3 open (rev naming, line/col residency, work-rev identity). Found the depth2 ref pair of bugs en route (depth2_ref_fix owns). Staged order: depth defect gates everything
```

## depth2_ref_fix

Source: `v6/prolog/ARCH.pl:824`

```text
LANDED 2026-07-30 (merge 2e2b983b, opus lane; coordinator re-ran EVERY leg incl the fail-first at the fixtures-only commit: 176 PASS/10 fail red before the fix). Relation-shaped term over a ref column = SUGAR for the canonical obj(SortedPairs) a world arrival already produces, rewritten once (0_relation_pattern.pl) before anything stores or unifies, so rule-built and world-arrived values are the same term at EVERY depth; depth-N needs no depth-N machinery. Emitter lowers to one __ref_<T> dictionary atom per level, memoized by term identity, ordered before expand_decode_rules so both dereference spellings compose. Shared unsupported construct relation_pattern_not_a_relation_value replaces an ACCIDENTAL catch (json_extract of an INTEGER endpoint only caught when the phantom TEXT expression happened to land on an int column); 2 compiler-only unsupported constructs name where the rewrite does not enter (under not/1, edge statements). Conformance 175->186, plunit 200, text door 131/131/0, sweep 131/129/0-wrong both modes, tsv2 92. SABOTAGE WORTH KEEPING: disabling the memoization leaves every fixture and the whole sweep GREEN while the depth-2 span insert grows 3->5 joins/arm; only the EXPLAIN hop-count test sees it. Old row: IN FLIGHT (opus, lane/depth2-ref-fix): relation patterns at depth>=2 silently empty in emitter (json_extract on INTEGER __id), chained decode depth-2 dead in oracle. Fail-first at depth 2+3, cardinality 0/1/many, EXPLAIN SEARCH receipts, shared named unsupported construct for out-of-scope shapes
```

## golden_flex_e2e

Source: `v6/prolog/ARCH.pl:825`

```text
LANDED 2026-07-30 (merge on main, opus lane; coordinator re-ran green + golden-flex + sweep + its OWN coverage sabotage, and re-ran golden-flex on the MERGED tree beside depth2: HOLDS, conformance 186). One .dl6 exercising 36 of 48 registry constructs + 12 named absences, each absence needing its reason in the golden header AND agreement with the registry status. Graded at 0/1/100/perturbed: oracle == incremental == naive on BOTH tick log and final state (6/45/1791/1802 final row groups). Served e2e leg: program POSTed as text, arrivals POSTed, rows read from /idb, real printf subprocess host, served tick log byte-identical to the oracle. golden-flex is wired into `green` itself, not merely green-all. COVERAGE SABOTAGE (coordinator's own): renaming latest( in the golden exits 1 naming latest/1; restore goes green. Also fixed en route: serve_lifecycle_idb_read_race (close the http server BEFORE disposing the program; the receipt holds an SSE client open and advances the interval bind on a virtual scheduler so a tick must touch sqlite after close began) and hostdecode_hardcoded_port_collision (17 hardcoded ports gone, port 0 + read it back). Old row: IN FLIGHT
```

## json_syntax_arc

Source: `v6/prolog/ARCH.pl:828`

```text
LANDED 2026-07-30 (merge 62f9ce84, opus lane, coordinator re-ran JSON_SYNTAX_LAB 25 PASS): cards only, parse_dl.pl untouched. Both constructs the recovery doc graded needs-new-surface have EXACT json1 lowerings (key capture = json_each(key,value) zero new SQL; ** = json_tree, fullkey = v4's dropped path bind free). Literal grammar IS the pattern grammar minus holes; key axis pattern-only forever. Lists: json carrier wins 5/5, json_list(T) = typed view, checker delta 4 clauses. RULED same day: json_key_hole_marker = dollar. 12 cards still open. Old row text: IN FLIGHT lab (opus, lane/json-syntax-lab): USER DIRECTIVES -- json rel type lowering to sqlite json1, native json5-ish {} literals with v3/v4-style holes ({ means json for now), list types + generics folded in. Lab returns exact spelling CARDS; nothing lands without user sign-off (no-unsighted-syntax law)
```

## openapi_codegen_spine

Source: `v6/prolog/ARCH.pl:829`

```text
LANDED 2026-07-30 (merge bf71c5b6, opus lane, coordinator re-ran receipts.sh exit 0): 5 routes/5 ops/14 responses/14 schemas from prolog facts; four-source parity gate (spec vs ROUTE_LIST vs parsed dispatch branches vs a LIVE server's own 404 list). KEY RECEIPT: drop one route fact and the spec STILL passes Redocly validation while the gate goes 0/3 -- a validator cannot catch a lie by omission. Buy: openapi-typescript dev dep + Redocly via dlx + progenitor post-flip; direct axum emission over utoipa. 11 cards open. Old row text: IN FLIGHT lab (opus, lane/openapi-codegen): USER DIRECTIVE -- one openapi spec generated from prolog facts drives ts server now + rust axum post-flip + CLI; generalizes the cli_command/3 inventory-parity mechanism to HTTP routes; buy research first; emit_openapi.pl prototype + parity gate red/green receipt
```

## rx_oracle_harness

Source: `v6/prolog/ARCH.pl:830`

```text
LANDED 2026-07-30 (merge, opus lane; coordinator ran run.sh itself: RXORACLE HOLDS 8/8). Same scenario written twice, leg A literal rxjs importing NOTHING from the repo, leg B bash only through bop serve + curl. Line format <step> <name> <sign> <payload>; the clock is the STEP not the tick, batches land at step midpoints, an event within guardMs of a boundary fails LOUDLY as a straddle (that guard caught the agent's own driver drifting 40ms/step). 4 declared normalizations, and N3 (drop sprefa's del lines) is opt-in because OPTING IN IS THE FINDING: rxjs has no retraction channel. Verdicts: mergemap EXACT, latest/keyed MODULO N3, same_tick_collapse + scan_state_feedback + host_concurrency + switchmap DIVERGE, unsubscribe INEXPRESSIBLE. THE CANCEL ANSWER, measured: sprefa does not cancel, it MEMOIZES -- liveDemand$ reads delta.add and nothing reads delta.del, so no cancel site exists; the loser runs to completion, its answer lands durably, and re-demand is a same-tick cache hit where rxjs re-subscribes and re-runs. Discriminating outside observation = TIME the re-demand
```

## rel_as_value_lab

Source: `v6/prolog/ARCH.pl:831`

```text
LANDED 2026-07-30 (merge, opus lane): what the user means by rels-as-values (pass a REL into an arg slot, higher order) is NOT what the relation-pattern feature does (construct + destructure ref columns, first order). Coordinator had mislabeled it. Findings: reading is sugar, WRITING is not (the head term is the only constructor for a ref column, __id has no surface spelling); the ask is ALREADY RULED by locked(higher_order_runtime_boundary) + locked(higher_order_lowering) = Souffle components + compile-time monomorphization, and ALREADY PROTOTYPED in labs/generic_scan_instantiation (8/9). 6 candidate spellings: 2 expressible, 1 ugly-expressible, 3 INEXPRESSIBLE AND SILENT (call/N has no registry row so edb_definition claims it as a phantom empty rel -- now defect D4). Coordinate crux: a content hash CANNOT name a rel (names members, changes every tick, a schema is not data); rel_definition_hash proves 3 axes (shape hash, semantic hash, Name/Arity as storage identity) and the users content-hash-plus-instance-hash IS the specialization cache key. Generics: nothing is parametric, that stands, rule-level generics are a separate axis
```

## scan_surface_ruled

Source: `v6/prolog/ARCH.pl:832`

```text
RULED 2026-07-30, no build needed: scan gets NO NEW SURFACE. The rel_as_stream lab returned an EMPTY tier-0 list across 19 constructs, so nothing about scan justifies syntax. Canonical spelling = keyed state rel (accumulator) + log rel (sequence) + a match block whose arms are ordinary |+> edge rules; graded byte-identical on both doors, and the match form already reads left to right per the ratified |-> |+> pair. UNBLOCKS FOUR LABS that were waiting on this one decision: scan_surface_composition, scan_match_reconciliation, select_scan_cache fold now; generic_scan_instantiation is held ONLY for the rule-generics card. Sugar comes later from repetition evidence, not from a guess. Still open beneath it: ordinal spelling (four rules / seq() sugar / engine-minted, the last carrying the two-doors ordering crack) and loop/select mechanics, which the user wants nearby-and-intuitive for lowering
```

## json_edge_body_unblock

Source: `v6/prolog/ARCH.pl:833`

```text
LANDED 2026-08-22 (feature/edge-body-decode, opus agent; coordinator re-ran conformance 444/0, plunit 1065/0, grade 444/340, cargo 158/0, ghcache ticks=10, goldens 6): decode/2 in an edge body lowers when the source column is json (lower.pl check_edge_decode_sources/3, compile_edge_guards/6); untyped or struct sources keep edge_body_needs_json_destructure, pinned at plunit edge_body_json_decode. Four ghcache _seen twins collapsed. Old row: REASON WENT STALE, unowned. lower.pl:931 keeps edge_body_needs_json_destructure because a compound arriving into an untyped column is stored as canonical term text and the encoding question was SLOT-TERM-STRUCT's. That slot WAS RULED 2026-07-29 (compound_storage = struct_as_rows). The guard seam for edge bodies already landed for negation/comparisons/binds (edge_body_constructs). User's argument, and it holds: json is not a time thing, so edge bodies should accept decode. Small, and it matters because edge rules are the state-machine idiom
```

## prolog_graph_cleanup

Source: `v6/prolog/ARCH.pl:835`

```text
LANDED 2026-07-30 (merge 71050af8, opus lane; coordinator verified the headline on the MERGED tree: 0.368s real). flagship-flow.dl6 compile 257.09s -> 0.22s; plan phase 256,950ms -> 62ms; plan inferences 6,011,087,004 -> 1,305,819; growth rules^4.864 -> rules^1.265; causal_dependency/4 39,046,545 calls -> 33,865. TWO causes, not one: SCC by all-pairs simple-path enumeration, AND recurrence_free_clock calling clock_scc/3 INSIDE ITS OWN NEGATION so a full component search ran per simple path. BUY VERDICT measured not read: library(ugraphs) wins for representation/transpose/neighbours/top_sort/cycles -- its transitive_closure/2 is EXACTLY the strict positive-length reachability the checker meant while reachable/3 is REFLEXIVE and would have been a silent wrong swap, and top_sort/2 fails on cycles and self-loops so it is the cycle detector too. ugraphs ships NO SCC, nor does any SWI library or pack (pack_list('scc') empty); composing SCC from transitive_closure is Warshall-cubic and tracks VERTEX COUNT not sparsity (27,082ms on a 1000-node CHAIN), so SCC alone is hand Kosaraju ~40 lines with the Warshall composition KEPT as the differential oracle; all four candidates agreed at 9 shapes. Tabling priced and REJECTED on measurement (5ms vs 1ms). HASH IDENTICAL: flagship sha256 28faeb19 both sides, corpus-wide 135 shared emitted modules 0 differ. Count rails pinned on exponent and ceiling, both sabotages red (un-hoisted 1.89, simple-path restored 3.83). LEFT UNSWEPT 11 sites with reasons; the toposort cluster (0_type_plane.pl, strat.pl, ARCH.pl, four hand-rolled Kahn variants) each fixes an EMISSION ORDER that byte-identity grading depends on, so top_sort/2 would change generated bytes for no measured gain, and none appears in the post-fix profile % IN FLIGHT (opus, lane/prolog-graph-cleanup): TWO COPIES of the same naive all-pairs-mutual-reachability SCC exist (compile/3_clock_check.pl:224-234 = the 255s hot spot, and labs/rel_definition_hash:296-315), library(ugraphs) ships with SWI and has ZERO uses in the repo, and :- table appears in exactly two files and in none of the type/clock/inference walkers. Brief order: buy-before-build analysis FIRST per the standing law (ugraphs vs SWI tabling vs hand Tarjan), then ONE memoized graph module both call sites move onto, then the wider sweep, then before/after numbers off the profiling bench plus a COUNT assertion so it cannot silently regress. User intent verbatim: types and clocks and inference unification memoized and close to each other
```

## scip_passthrough

Source: `v6/prolog/ARCH.pl:837`

```text
LANDED 2026-07-30 (merge 8f6a4377). SCIP coverage 17/43 -> 43/43 serialized fields; the whole Metadata/ToolInfo block was LOADED AND NEVER EMITTED. Cost priced rather than hidden: 204 docs went 150,640 rows/32.1MB -> 177,967/59.4MB and ~17MB of the 21.5MB growth is CONSTANT columns (syntax_kind is 0 in 123,655 of 123,655 rows). VERDICT: the engine cannot hold 100pct as a standing feed at 291k rows per 1,000 files against 74.2 files/s ingest with commit_ms dominant, so --scip-record KINDS filters AT PRODUCTION and a narrowed stream also skips reading the corpus. DIET MODULE RESOLUTION landed over the oxc specifiers ts.rs:883 already captured (one step, not zero): scip 764 edges/755 agree/recall .992/prec .988 vs diet 761/761/1.000/1.000, and the lane stated the honest reading itself -- diet's 1.000 is agreement between TWO SYNTACTIC SCANNERS, not correctness; the 9 scip-only edges reach a declaration through an INFERRED TYPE with no import statement, which no syntactic resolver closes without a type checker
```

## stale_labs_sweep

Source: `v6/prolog/ARCH.pl:838`

```text
LANDED 2026-07-30 (merge 74200375). 5 folded, 2 kept, and ghcacher_tick_golden PROMOTED after the lane refused the coordinator's framing: it was filed as debt and is a WORKING GATE nobody ran, now in v6/tsv2/goldens/ and wired into green-all with a sabotage receipt. CLAIMS NOW KNOWN WRONG: the bootstrap lab did NOT self-host while its plan doc still reads like a live 7-phase arc; interning is a 2.44x win ONLY when names repeat and a 1.2% LOSS when unique, which applies to a SHIPPED dep (sprefa-store uses lasso) and was written down nowhere; rel_value_unification's two red checks were correct behaviour with only the ENCODING moved; bidirectional pattern_value/3 is a toy-scale claim declined at real scale. HANDED FORWARD: swipl packaging unanswered (otool shows @rpath/libswipl.10.dylib, a distributable needs a static SWI build, and this is the ONLY measurement of the bundled-with-rust tier)
```

## effect_chain_batch

Source: `v6/prolog/ARCH.pl:839`

```text
LANDED 2026-07-30. N stages = N+1 ticks, one hop per stage. Same-tick chaining forbidden TWICE (compiler refuses two host atoms per body; runtime re-enters only via engine.submit) and the general reason is that it would require suspending the fixpoint on I/O. Cost is PER ITEM: ticks = seeds*stages + 1. Batching 7-to-1 verified (700 demands -> 100 spawns) and it collapses ticks 700 -> 100 too; compatibility = same tick frontier AND sprefa_extract AND byte-identical template AND identical input values; a plain sh host CANNOT BATCH AT ALL. V5 COLLECT IS FAN-IN OVER DISTINCT VALUES, today's grouping is FAN-OUT DEDUPE OVER IDENTICAL VALUES, opposite requirements. Planning is already fully comptime and is NOT the bottleneck; every gap is a missing FOLD, and one list-valued aggregate head makes collect expressible with zero new effect machinery
```

## group_concat_silent_miscompile

Source: `v6/prolog/ARCH.pl:840`

```text
OPEN DEFECT F9 from effect_chain_batch, coordinator-reproduced: group_concat(x) in a head COMPILES CLEAN at 200 and is not an aggregate. It stores the literal text group_concat(1), one row per input, silently. That is the exact spelling a cold author reaches for when they want v5's collect RECONCILED 2026-08-02: superseded by oracle_body_gate (refuse not_implemented) then ordered_aggregate_arc (done 2026-07-30): group_concat/2,/3 + json_group_array shipped; devlog_rail dogfoods it. Not reproducible as filed.
```

## host_column_shadows_runtime

Source: `v6/prolog/ARCH.pl:841`

```text
OPEN DEFECT from staged_writes, coordinator-reproduced side by side: a host declaring an input or output column named ordinal or witness_digest compiles CLEAN, the compiler renames ITS OWN runtime columns to col1/col2 to dodge the duplicate while serve/1_hosts.ts project() fills BY LITERAL NAME. Empty witness, PK collapsed to ("",""), multi-row answers cut to one, dead demand-to-response join, program derives nothing, no unsupported construct and no trace. Fix is a load-time unsupported construct, not a rename. Bit its own lab twice in one sitting RECONCILED 2026-08-02: unsupported construct shipped in 54dc7604 and fires today; 6-ordinal.dl6 (staged-writes lab fixture) predates it and now trips the unsupported construct in receipts.sh phase 0 -- fixture update is a SEPARATE unowned task.
```

## staged_writes_lab

Source: `v6/prolog/ARCH.pl:842`

```text
LANDED 2026-07-30 (merge 787d994d), the tool's NAMESAKE arc graded. Marker writing already works and stops in three measured places: a host's input is a ROW while a write's payload is a RELATION with no string aggregate available, so 3 staged lines = 3 spawns = 3 whole-file rewrites ordered by DELTA ORDER (v5 folded rows in the engine, one write per file per tick); the engine is BLIND to the disk it wrote because effect identity is content-addressed over the DEMAND only; writes are at-least-once. THE do-not-advance-until-answered RULE IS ONE concatMap AND IT BUYS ORDERING, NOT DURABILITY -- kill -9 between the disk write and the answer replayed the write on restart. Its real costs: the tick log stops being a pure function of (program, schedule) and becomes a function of effect WALL TIME, ending byte grading for any program that writes; one slow write freezes the whole engine; a host that curls the engine becomes a hard undetected deadlock. Dry-run-as-rows works today and beats --fix because armed(zone) is an ordinary row RECONCILE NOTE 2026-08-02: "no string aggregate available" was true when graded and is stale since ordered_aggregate_arc landed group_concat the same day.
```

## ts_lowering_review

Source: `v6/prolog/ARCH.pl:843`

```text
LANDED 2026-07-30 (merge be97647c). 2 CRITICAL: one tick fault PERMANENTLY kills the engine and the served process (3_engine.ts:176 re-throws into the shared concatMap behind ticks$, and 4_http.ts:315 states the opposite law in its own comment; no gate catches it), and the HTTP arrival boundary checks row LENGTH never row SHAPE (row:"ab" kills the server; row:[{a:1},[2,3]] returns 200 and STORES [null,"[2,3]"] into TEXT NOT NULL where it persists). 3 HIGH: unescaped shell injection in the {col} template splice with spawn(shell:true) and the probe CREATED A FILE ON DISK; host subprocesses run at concurrency 1.0, the structural cause of 40.68 files/s vs v5's 3,540.93 same-run; the ordered/pre family is constant+2n statements per tick with no count assertion. 4 of 8 standing TS laws violated. pnpm typecheck is RED with 4 errors and NO gate runs the compiler
```

## prolog_main_review

Source: `v6/prolog/ARCH.pl:844`

```text
LANDED 2026-07-30 (merge 9374bf5b). 15 proved / 6 suspected. F1 the ORACLE HAS NO CLAUSE for combine/variadic or next/1, both registry LIVE, so term-form combine derives zero rows while the compiler emits a real cross join; hidden because parse splices at parse time, print_dl ERASES the spelling, and no term-corpus fixture uses either, while the oracle's own stratifier DOES handle it. F2 zip/subscribe/complete/unsubscribe/error silently derive nothing on the oracle and are refused by name on the compiler. F3 a level-head expression contradicting its declared column type is checked by NOBODY (bool and float get a CHECK in that same predicate; int, text and ref get none). F4 today's D3 fix fires only when BOTH columns are declared. F5 CORRECTS THE PROFILING HEADLINE: dominant_phase emit and rules^0.964 came from a ONE-ATOM SYNTHETIC at 0.5ms resolution; all four real programs are still PLAN-dominant and the same generator at n=200 gives rules^1.84, with the hot spot at analyze.pl:310 re-walking every rule body once per (ref,column), 16,315 walk_node calls for a 181-line file
```

## null_implementation_plan

Source: `v6/prolog/ARCH.pl:845`

```text
LANDED 2026-07-30 (merge 660fedcb). 10 ordered steps, each with a fail-first receipt and a reversibility mark; 18 emitted SQL sites change with line numbers, plus 10 MEASURED NO-OPS not to touch. Oracle door = null_value(sql), a reserved ground compound, because engine.pl decides row identity with memberchk/==/sort/ord_subtract/set_diff_delta, all TOTAL on a ground term, so THE ENGINE NEEDS NO CHANGE AT ALL; an atom is unusable since none has 84 corpus uses as text and body.pl:147 reads it as JSON absence; bool_lit(true) is the precedent; a T? column is a disjoint sum on the VALUE axis so TICK-MODEL's B/N/Z are untouched. FOUR CORRECTIONS to the null lab, incl BARE null CURRENTLY PARSES AS A FRESH VARIABLE so repo_latest(Repo,null) silently means any value, and present(X) is not caught by unsupported construct-by-absence so a typo becomes a silent empty EDB. Step 6 is the estimate risk: narrowing would be the FIRST REBINDING OPERATION in this compiler
```

## option_vs_null_lab

Source: `v6/prolog/ARCH.pl:846`

```text
LANDED 2026-07-30 (merge 0f521736), deliberately NOT closed, 9 cards none recommended. The user's three-variant read WORKS TODAY via json_type/2 with zero new semantics, and the negative control is the finding: the SHIPPED json_extract IS NOT NULL predicate reports 5 present where json_type reports 6, so ONE DOCUMENT IS SILENTLY LOST TODAY. An Option has FOUR observable states; the fourth is row absence and no variant expresses it. THE EXPLOSION WORRY INVERTS: at 100k/10pct absent, Option is 1.57x bytes while the NULLABLE COLUMN ON A DERIVED REL is 2.14x and the LARGEST, forced off WITHOUT ROWID onto rowid+UNIQUE. THE WALL: an Option from a DERIVED rule does not compile, since 0_enum_expand.pl attaches keyed(...) to every variant rel so the outer-join shape hits keyed_level_head; class (b), highest-value card. SOUFFLE: no general null, nil only in record-typed columns, no json type, shape declared up front so a maybe-missing key CANNOT ARISE; issue #2315 leaves negative ADT branch matching unshipped. THREE DISAGREEMENTS with the null lab in writing, including its own refuted hypothesis: IS NOT DISTINCT FROM does NOT lose the index, identical SEARCH plans on both builds
```

## defect_wave_0730

Source: `v6/prolog/ARCH.pl:847`

```text
LANDED 2026-07-30 (merge 8d71f543, opus lane; coordinator re-ran on the branch: just green exit 0, sweep 133 compiled/131 identical/0 wrong, GOLDEN FLEX HOLDS, conformance 186 -> 193). All 8 fail-first. D1 parseWhitespace mangled any 2-row host answer on the SHIPPED enumerate_at path, red receipt straight off the trace: decoded rows per demand [0,1,1,3] where it must be [0,1,2,3] (right at 1 and 3, wrong at 2). D2 backslash deleted twice; rule now stated (\n \t \r escape, \\ is one backslash, own quote is itself, every other \X is two chars) and it exposed a THIRD conflation -- three emit_ts joins built multi-statement SQL from the two-char sequence backslash-n and leaned on the template literal to convert it, so escaping correctly turned 11 fixtures into SQLITE_ERROR unrecognized token; they join with a real newline now. D3 a variable carrying text into a ref column reached INSERT OR IGNORE; fixed as a LOAD-TIME check because the contradiction is fully decidable from prog/2. D4 silent call/N, and its negative leg matters: `call` is a legal relation name and the flagship declares call/2, so the trigger is an UNDECLARED UNHEADED call/N (the first cut broke the flagship). D5 depth-1 join regression, and the RECEIPT STANDARD WAS RAISED as instructed -- relationDepth.test.ts now asserts every row source BY NAME in FROM order per delta arm, not just access method, with a sabotage receipt in the header. D6 refuse on BOTH doors rather than lower, with a real reason: a relation value under not/1 needs its dictionary joins scoped INSIDE the NOT EXISTS (hoisting inverts the answer) and an edge rule has no dictionary-join seam at all; deleting the shared unsupported construct is how the capability lands later. D7 norm/1, where the INVERTED golden step fired by design and the agent did all three things its message demanded. D8 float and bool now spell in sh and bind decls; premise correction, struct type names did not work there either (only host OUTPUTS resolved them), and a second half remains one layer down where the witness-digest concat refuses them, which is an emitter agreement decision not a widening. BANKED, unfixed: zip/2 is the same class as D4 (compiler refuses by name, oracle silently derives nothing, reserved-word gate on one door only) % IN FLIGHT (opus, lane/defect-wave): 8 defects, fail-first each. D1 parseWhitespace mangles any 2-row host answer on the SHIPPED enumerate_at path; D2 backslash silently deleted in .dl6 string constants twice (parser + emitted template literal); D3 a VARIABLE bypasses relation_argument_violation and writes text into an INTEGER PK column; D4 silent call/N; D5 depth-1 join regression from 472320f4 (self-join through __ref_<T>, 2 rel_value_unification checks already red); D6 sol burrs incl the two unsupported constructs that are unfinished work the oracle executes fine; D7 norm/1 oracle-vs-emitter divergence (fixing it turns golden-flex step 5 red BY DESIGN, an inverted receipt); D8 float/bool unspellable in sh and bind decls
```

## prolog_compile_profiling

Source: `v6/prolog/ARCH.pl:848`

```text
LANDED 2026-07-30 (merge 4dd7e230, codex sol). plan phase = 255,333ms of 255,490ms at 6.01e9 inferences; hot predicate clock_check:graph_reachable/4 under clock_scc/3; growth exponent 4.899. Root cause read and confirmed by the coordinator: SCC by ALL-PAIRS MUTUAL REACHABILITY with simple-path enumeration and no memo. A 27-rule real program took 255s vs 3.6s for a 30-rule chain, so graph SHAPE dominates size. statistics/2 in setup_call_cleanup/3 + library(prolog_profile); trace interception rejected at a measured 5.3x; off path proven free by sha256 equality. JSONL under DL_PERF_LOG matching the runtime shape so numbers survive the rust flip. Old row: % IN FLIGHT (codex sol, ../sprefa-codex-plprof): compile cost is steeply superlinear, 7.9s at 60 statements vs >232s at 117, cause unknown and misdiagnosed as a hang twice. Must use SWI's OWN tooling (statistics/2, library(prof), setup_call_cleanup boundaries) and SAY WHY each was chosen or rejected; output is JSONL matching the existing DL_PERF_LOG shape so the numbers survive the rust flip; shell bench reproduces the N-vs-time curve from outside the process; deliverable names the dominant phase and its growth shape
```

## teardown_flatten_lab

Source: `v6/prolog/ARCH.pl:849`

```text
LANDED 2026-07-30 (merge d966b407, opus lane; coordinator ran receipts: TEARDOWN LAB HOLDS, 15). Door 1 was ALREADY RULED and unimplemented since 2026-07-27: effect_abort's own text says demand-row deletion IS the abort signal and names the unbuilt site. 1_hosts.ts:175 already returns () => child.kill() and it has never been called. Four flatteners graded on one /ticks stream, one operator changed. merge and concat are the SAME PROGRAM (which inners die vs how many run are different axes); switch needs NO key because del carries the decision, so teardown-on-del is MORE GENERAL than switchMap; NO ordinal needed. Teardown = 4 sites in serve/1_hosts.ts. Old row: % IN FLIGHT lab (opus, lane/teardown-flatten): grade all FOUR flatteners (concat/merge/switch/exhaust) as ordinary rules, on the thesis that identity and flattening are two axes we mashed into one. Is teardown one site or many; does door 1 obey the effect_abort ruling or need it amended; is concatMap serialization the bigger cost than the missing cancel; where does the ordinal live
```

## rel_as_stream_lab

Source: `v6/prolog/ARCH.pl:850`

```text
LANDED 2026-07-30 (merge 7d040ab0, opus lane; coordinator ran receipts 10 PASS 0 FAIL). TIER-0 LIST EMPTY across 19 constructs. A stream is a VIEW OF A TABLE PLUS A NAMING CONVENTION; the single-rel lock is not fought because log_on_level_headed_rel already forces a reader's projection to be a table. rel-as-stream = log rel + ordinal from a keyed cursor read through pre/1, byte-identical both doors. Backpressure needs ZERO constructs (derived watermark + visible dropped rel). keep(count(0)) = deliver-and-forget, so the rx Subject family is a two-word decl matrix. zip = equijoin on ordinal, bufferCount = integer division. CRACK: the two doors already carry different internal orders (engine.pl:357 global counter vs lower.pl:2275 per-table rowid), ungraded because unobservable; surfacing an ordinal picks one and the pick becomes a contract. Old row: % IN FLIGHT lab (opus, lane/rel-as-stream): can a rel BE a stream with existing mechanics (log + ordinal), is a stream a different THING or a VIEW of a table, and the TIER-0 TEST on every wanted construct (sugar / new lowering / genuinely new semantic). Must describe a lowering target several runtimes could hit, not "whatever rxjs does" -- the user's warning is that hosts already got welded to concatMap
```

## external_oracle_scout

Source: `v6/prolog/ARCH.pl:851`

```text
LANDED 2026-07-30 (merge d9b808bf, codex luna analysis-only): answers 'does sprefa-extract go against a madge oracle like v5'. v5 YES (oracle_madge.rs grades 8 rels from examples/madge.dl against madge json/circular/leaves/summary/depends/warning) but ALL 14 v5 oracle test fns are #[ignore]d = 0 run by default. v6 engine legs have NO external oracle; v6 EXTRACTOR does (golden_parity.rs runs scip-typescript/scip-go/rust-analyzer as ordinary non-ignored ratchets). Real gap = no v6 resolved TypeScript dep(src,dst) rel to diff (no ts module resolver in the extractor, types.rs:587-603); the diff harness is the easy half
```

## v5_parity_spelunk

Source: `v6/prolog/ARCH.pl:852`

```text
LANDED 2026-07-30 (merge 754889b5 -- the lane the coordinator relayed and forgot to merge, caught by the user asking; finding filed). v5 is SELF-DESCRIBING (op_catalog/fn_catalog/rel_catalog are builtin rels) so the inventory comes from ASKING v5 through an sh host, line-regex leg graded against that. 28 ops / 16 fns / 112 rels; v6 covers 20, partial 4, absent 132, ZERO of sixteen scalar functions. scan() appears in 105/129 examples and 29/33 rails and is the shell-glob spelling. Plus the git-fact diags rail class (enumerate vs enumerate_at antijoin = new-file diags)
```

## json_wiring

Source: `v6/prolog/ARCH.pl:854`

```text
LANDED 2026-07-30 (merge before c04b6cfd, opus lane; coordinator re-ran on merged main: green exit 0, conformance 201/0, TEXT_DOOR 141/141/0, plunit 236/236, sweep both modes 141/139/0-wrong). Brace spelling settled BY MEASUREMENT: _{} and Tag{} are SWI dict syntax the term door would never see as {}/1, so bare {...} wins and both dict forms RESERVED (tagged_brace_reserved). json became its OWN STORAGE KIND (was a text alias, which erased the dispatch bit). Exact keys 0 joins / spread+key-capture 1 json_each / ** 1 json_tree; delta arm reads json_each off the frontier, zero delta-side scans; guards FIRST in WHERE (SQL states no AND order). 20 of 27 archive examples verbatim through the production text door incl gh-cache both spellings. KEPT REFUSED with measurements: json agg heads (json_group_object row order vs sorted keys; no ORDER BY slot in the flat aggregate SELECT) and edge-body json (blocker located: analyze.pl:edge_goal_unsupported/4 receives only Prog, no relplans, cannot tell json from struct)
```

## ts_critical_fixes

Source: `v6/prolog/ARCH.pl:855`

```text
LANDED 2026-07-30 (merge bd0d567c, opus lane; coordinator re-ran on merged main: green exit 0, sweep 141/139/0, MEMORY SOAK HOLDS, serve-leak-soak clean). F1 was WORSE than reviewed: the tick fault closed the LISTENING SOCKET through runProgram's merge (ECONNREFUSED). F2's first draft was too strict, golden-flex caught it (struct values arrive as whole JSON objects; ref columns accept any JSON value, only absence refused); validation lives at the hand-written boundary, the only trust boundary and the only holder of relColumns/relColumnTypes for a named 400. F3 all 5 injection payloads executed pre-fix. F5 FIXED 2026-08-23 by ordered_tick_recompute, not by pre_occurrence_loop: run_tick keeps a dirty set off the rows_changed every write already returns, so a level recomputes only when a rel it reads moved and the snapshots read the moved rels only. ghcache statements/tick 1890 -> 443 (t0, which rebuilds every level because a killed process can leave one stale), 1902 -> 367 (t5, 36 of 100 levels), 1881 -> 59 (t9, zero arrivals, 6 levels), tick log byte-identical. typecheck WIRED INTO GREEN (npm test strips types; the package sat 4 errors red with no gate noticing)
```

## time_plane_lab

Source: `v6/prolog/ARCH.pl:857`

```text
LANDED 2026-07-30 (lab commit dc9a5030, verdict plans/2026-07-30-time-plane-unification-verdict.md, lab-death executed by the retention landing lane, last copy f044ce06; coordinator re-ran receipts.sh 7 PASS 0 FAIL). H1 (log = rel + time column as sugar) REFUTED ON MECHANISM: a set rel absorbs an identical second arrival as a no-op BEFORE any rule runs, so log's job is the ENGINE minting row distinctness and no expansion written in rules can mint it. Blast radius 6 die / 6 move / ~105 survive / 2 new; seq-as-visible-column = 80-fixture regrade + reorders every tick. H2 (created/updated_at_tick) already expressible in two edge rules (now/1 + pre/1 + not/1); the naive one-rule spelling is silently updated_at wearing created_at's name; sugar if ever = opt-in per rel (automatic costs 7.5 bytes/row). H3 history: shadow log +15.2% vs rel-as-log +72.4%. Slots: seq_scope = per-rel (the ORACLE is the side that must change; counter = next_seq/3 engine.pl:379-381 + Seq0 thread :291-313); updated_at = keyed rels only
```

## retention_minus

Source: `v6/prolog/ARCH.pl:858`

```text
LANDED 2026-07-30 (lane commit d0a6bbd4, merged; coordinator re-ran conformance 214/0 + TEXT_DOOR 150/150/0 on branch, green exit 0 + sweep BOTH modes 150/148/0-wrong + compile-speed OK on merged main). Retention prune = VISIBLE minus delta on both doors; finalize-over-log FIRES (fixture supersedes SLOT-LOG-FINALIZE-REFUSAL + stream card 4a + consumption-arms 17). THE LAB UNDERCOUNTED: THREE suppression sites not two -- emit_ts diff_local_line/3 emitted literal del:[] for kept rels, the NAIVE REFEREE's copy, invisible to a single-emitter-mode lab sweep (method lesson banked). Exactly ONE artifact line regraded corpus-wide. +5 fixtures incl the log/set occurrence-identity pair; retention_count_prunes_oldest gained its missing deltas/2 leg. TICK-MODEL 5.1 splits R7: occurrence in N (no minus, ever) vs storage row in Z (keep alone)
```

## golden_gates

Source: `v6/prolog/ARCH.pl:859`

```text
LANDED 2026-07-30 (lane commit c00043be, merged; coordinator re-ran all four gates in-worktree + green and compile-speed on merged main). green-all grew compile-speed (INFERENCE-COUNT ratchet, 3 runs byte-identical at 2,120,677 while wall wobbled 7->11ms; sabotage scan = +899.8% named FAIL where wall moved 7->12ms), scale-floor (statement-count SET across cell sizes 10x apart; single-size assertion cannot tell fixed overhead from a per-row loop), multirepo-golden (program 5's v6 leg: v5 version-skew.dl UNMODIFIED vs v6 port + a third opinion reading go.mod bytes, 4/4 rels byte-identical 0 unclassified; named gap dep_ver min/max over text refused), rtkq-golden (EXISTED COMPLETE since 51ba4161, never wired into any battery; sabotage flips on byte spans). Findings: v5 dep_ver min/max REVERSED on pkg/errors (README, ungraded); rtkq runner under tsv2/labs/ = protocol debt; gen/scale_generated.ts stale (gen_staleness_gate class)
```

## json_flex_lab

Source: `v6/prolog/ARCH.pl:860`

```text
LANDED 2026-07-30 (lab commit 7612fdc1, merged; lab-death done, last copy 6dde7f9a; verdict plans/2026-07-30-json-flex-verdict.md; coordinator re-ran conformance 221/0 + TEXT_DOOR on branch, sweep BOTH modes + green on merged main = 226 fixtures, 162/160/0-wrong). Semantics HELD under every value kind; FOUR invisible defects under them fixed fail-first: oracle tick log was not JSON (~4| hex-escape column stop, THREE encoder copies), text-column json object -> SQL NULL, first-char structure sniffing (json now its own IRowColumnType end to end), json_canon binding a free variable to {}. ref columns needed the same encoder arm (memoized __rendered NOT key-sorted -- refutes struct-as-rows canonical-at-intern-time, card C6). JSONTestSuite y 95/95 / n 187/188 / i 31-4, zero crashes, 127 round-trips idempotent. OPEN CARDS (11, measured): Q2 null-collapse SHARED by both doors (unilateral fix creates divergence); oracle json null = atom none makes text "none" unreachable oracle-side; dup keys throw-vs-last-wins; 1e999 -> null; wide-int RangeError cliff at 2^53-1; nesting cap 1000/1001. Compile-speed gate made its FIRST CATCH on this merge (+11.7%/+14.2% emit inferences, accepted via --write-baseline as the correctness cost)
```

## stream_cards_ruled

Source: `v6/prolog/ARCH.pl:862`

```text
RULED 2026-07-30 (rulings.pl tail, six entries): seq(name) column-type sugar (1b; the @ binding 1c is DEAD by user word), zip stays reserved with a join-naming message (2b), backpressure = watermark-gated writer + visible dropped rel (3a; CSP banked as pending-log + one-per-drain-tick + clock-joined drain, coordinator PROVED the composition cold in csp.dl6 -- bop check exit 0 + oracle log with visible keep(count(3)) eviction, program preserved in the csp-idioms header), latest-over-log = load unsupported construct naming max(Ordinal) (5b), no stream decl word (6a), cross-rel drain order = documented non-contract (7a). Card 4 was superseded by retention_minus (finalize-over-log fires). WIRING for 1b/2b/5b/7a rides the csp-idioms lab's census evidence
```

## csp_idioms_lab

Source: `v6/prolog/ARCH.pl:863`

```text
LANDED 2026-07-30 (lab commit bb84a26b, merged; coordinator re-ran receipts 18 PASS; verdict plans/2026-07-30-csp-idioms-verdict.md; lab dir ALIVE pending the fold lane, protocol debt accepted on the time-plane precedent). All 9 idioms expressible, 8/9 byte-identical both doors. ROOT DEFECT W1/W2: read-aggregate-compare-write guards enforce capacity per TICK BOUNDARY never per row (worker pool double-assigns silently, semaphore breaches K same-tick, both doors agree so no diff catches it). W3 count() no zero = naive semaphore grants nothing forever, clean compile. W4 NEW divergence: aggregate publishes transient mid-carry value off a derived trigger (2 oracle / 6 served deltas, review-A4 class). Census: 78% verbatim repeats; seq sugar confirmed WIDENED (numbering+view+selector = 40 of 94 rules). Parse errors print raw char-code lists (worse than B4). 8 fixture candidates (4 fail-first) in the verdict
```

## oracle_body_gate

Source: `v6/prolog/ARCH.pl:865`

```text
LANDED 2026-07-30 (lane commit fec109db, merged; coordinator re-ran conformance 240/0 + TEXT_DOOR 168/168/0 on branch, green + sweep both modes 168/166/0 + compile-speed 0-regressions on merged main). F1 combine/next = REAL solve/2 clauses (compiler lowering byte-identical to bare conjunction; two silences closed: solve splice + trigger_items splice_bare -- edge rules over combine were statically dead). Erasure corrected: parse_dl desugared splice at parse time, the two doors held DIFFERENT terms for one source. F2 reserved rows refuse BOTH doors (reserved_body_word in 0_program_check, walk in 0_body_walk); refused rows stay oracle-executable on purpose; EDB boundary preserved. group_concat = refuse(not_implemented) not the aggregate class. Host shadow mechanism CORRECTED: dedupe_terms folded the col_type term (column VANISHED); host_column_shadows_runtime unsupported construct + drift guard. compile.pl wraps shared pre-pass unsupported constructs at program_plan. Load checks cost max +0.2%
```

## extract_t2_lab

Source: `v6/prolog/ARCH.pl:866`

```text
LANDED 2026-07-30 (lab commit 447ee181, merged; coordinator re-ran receipts 19/0 HOLD; verdict plans/2026-07-30-extract-t2-verdict.md; lab dir alive pending fold). Buy table executed, ZERO bespoke parsers (pbjs/graphql-js bought verified; protoc-descriptor + quicktype REJECTED measured). All 8 algebra rows proven on real docs; 5 dl6 reifiers, 951 facts, byte-identical oracle vs served; calls_shape cross-repo lint REAL (undeclared_shape_dep sabotage 2->5); statements flat across 20x doc size, 800 repos ~32s projected. Q5 fidelity exact but rebuild is python: COMPILED json plane is READ-ONLY (4th assembly-gap sighting). D2 SHARPEST OPEN: decode over heterogeneous values has NO correct column type -- json column SILENTLY DROPS scalars via INSERT OR IGNORE over CHECK(json_valid). A1 ** unanchored invented 3 enum variants; A4 underscore keys parse as variables (GraphQL __schema)
```

## self_map_rail

Source: `v6/prolog/ARCH.pl:867`

```text
LANDED 2026-07-30 (lane commit 7d4cdeb5, merged; coordinator ran self-map twice = identical sha, regenerated on merged main). v6/ARCH-MAP.md: 4 mermaid diagrams 80-95% derived in dl6 (phase successor chain via min(), registry axes, task frontier antijoins, own 46-rel dataflow through real analyze.pl); LIVE MODE works (inserted phase row rewired the successor chain on disk). FIRST-RUN CATCH: ARCH.pl carried 2 duplicate-state task rows invisible to just arch; task_state_conflict surfaced them; coordinator resolved (deletions annotated in place). Gaps dogfooded: NO STRING AGGREGATE caps every dl6 share (mermaid text lives in self_map_render.py), probe-output guard unbound_head_var. No staleness gate on ARCH-MAP.md yet (named)
```

## seq_sugar_wire

Source: `v6/prolog/ARCH.pl:869`

```text
LANDED 2026-07-30 night (merge d2dadb93, user word "approve seq"): Ordinal := seq(name) via shared 0_seq_expand.pl, byte-identical to the 4-rule desugar both doors; pre/1 registry+golden+SYNTAX doc truth in the same lane; golden coverage 49 exercised / 12 absences. Coordinator emitter fixes riding it: ordered-tick retention triple + the rxjs-pipe-9-overload ceiling (12-stage chain typed unknown; template splits the pipe).
```

## selfmap_single_file

Source: `v6/prolog/ARCH.pl:870`

```text
LANDED 2026-07-30 night (merge 0d194c3a): RELEASE GATE ruling release_gate_v620 SATISFIED -- ARCH-MAP.md from ONE dl6 file, python renderer deleted, run-twice identical, 4 mermaid fences. Seven named unsupported constructs hit en route (each correct); compiler fix: group_expr guards the COMPILED SQL for GROUP-BY literals (:= 0 through coalesce default emitted bare positional 0). Named gaps banked: no escape spelling for a quote inside a text literal; batch-path sqlite errors carry no statement text.
```

## devlog_rail

Source: `v6/prolog/ARCH.pl:871`

```text
LANDED 2026-07-30 late (merge 4dd6b76a, ruling devlog_rail = approved_dogfood): DEVLOG.md from ONE dl6 program over chat_log/*.pl session ledgers -- sh host consults each .pl and emits JSONL, decode/spread into rels, per-session sections, group_concat('\n', Ordinal) assembly, one write host. Run-twice identical (worktree c4dfdb5e, merged main 639a2780, 206 lines). Coordinator fixes on the lane: clause/2 raises on static preds -> catch(M:Fact,_,fail); host args as position/value dicts; dedupe render rule; title ordinal -9e15. ROOT-CAUSED 35-min hang: /ticks is a STREAMING endpoint (200, holds the connection, zero bytes) -- any bare curl read of it blocks forever; quiescence = DEVLOG.md digest stability instead. LEDGER-SHAPE INVENTORY: lane_landed/in_flight/finding/answered/directive parsed; older free-form shapes skipped by name in devlog.dl6 comments. Named gap: /ticks streaming behavior undocumented, no snapshot twin endpoint (scripts must never bare-read it).
```

## golden_readiness

Source: `v6/prolog/ARCH.pl:872`

```text
LANDED 2026-07-31 (merge d0c03cba, opus worktree): v6/READINESS.md = the 9 stopping-point programs graded by timed run. ghcacher-golden was RED on main (stale read_schedule/2 vs type-directed dl6_oracle; gen/receipt staleness class) -- repaired; precommit-changed (program 3) + watch-scale wired into green-all. PRICED GAPS: v5 surface 132/156 absent (six clusters: gen 24, comment 20, match_ast 14, closure 17, scalar fns 14, repo/rev); crawl parity 84x, root cause named = NO ORG FAN-OUT SPELLING (v6 leg is a shell loop, not one program); tick-log format spec doc missing (S, two byte-agreeing impls); watch cost reported never gated (S).
```

## float_avg_arc

Source: `v6/prolog/ARCH.pl:873`

```text
LANDED 2026-07-31 (merge 13653a77 + intMode revert 0b7c2d37, codex luna): beta gate 2 -- float REAL/binary64 exact, shortest round-trip shared oracle/TS (js_float_text), avg delta-maintained sum+count, int_out_of_range boundary unsupported construct both sides, +5 fixtures (conformance 265, sweep 190 compiled). Bool column shape correctly skipped (nothing needs it; ruled row-presence).
```

## clock_legs

Source: `v6/prolog/ARCH.pl:875`

```text
LANDED 2026-07-31 (opus worktree): replay gate over 5 historical bug classes (A12/C2/not-in-arm CAUGHT -- not-in-arm as LABEL because a unsupported construct breaks the live json_typed_capture fixture; A4 out-of-scope by name with strengthened companion; tick-phase pre-existing), focused suite 25->30, clock_fact/5 proof facts, rank-3 SCC extension refuted by measurement. clock_check rulings now EXECUTED; the checker is buildable-on per the user's maybe-later.
```

## bench_cli

Source: `v6/prolog/ARCH.pl:876`

```text
LANDED 2026-07-31 (opus worktree + coordinator fixes 62ff636b): rust-course phase 0 -- language-agnostic CLI contract, buy-verdict (hyperfine secondary, /usr/bin/time -l for RSS), 11 timed cells byte-identical, floor gate. Coordinator restored the gitignore-eaten node_modules symlink (all 14 cells error'd silently at exit 0).
```

## bench_reference_tier

Source: `v6/prolog/ARCH.pl:877`

```text
LANDED 2026-07-31 (opus worktree, merge 8972dac1; coordinator re-ran the full rig in worktree AND merged tree, exit 0 both): ruling bench_reference EXECUTED -- identical_vs_reference verdict, tsv2(proven) referee, 3-condition promotion (oracle over-budget exit-124 ONLY this run, sweep-artifact breadth proof consumed-not-recomputed + sha-recorded, in-run currency all swipl-reachable cells identical), pass-2 reference log = a 6th separate invocation (the rust adapter's exact seam), final-state hash gates ALONE (sabotage c receipt), report exit-1 on wrong/ungraded/missing-proof + floor gate reads THIS run's csv. Standings 16 cells (11 swipl / 5 reference): s3 DNF gone (tsv2 8-10s/939MB under 512MB-naive-OOM history), stmts/tick flat 10k->100k (23 s1 / 37 s2). CONTRACT corrected by the lane: text-door byte-identity is 96/190 (94 differ = Initial facts have no .dl6 spelling; all 9 bench cases in the 96). Residual risk written not papered: a bug SHARED by tsv2 and a future rust engine has no third opinion at scale (mitigations: 190-fixture small-scale proof + final-state second reading). WALL DECOMPOSED by coordinator on user question: ~52% of the ~5min run = 5x30s swipl budget timeouts re-proven serially every run; parallel-timeout fix = zero-semantics harness change, s1_10k noisiest cell (280/627/279ms) named in section 7.1.
```

## oracle_scale_ceiling

Source: `v6/prolog/ARCH.pl:878`

```text
ANSWERED 2026-07-31 (merge 8972dac1): ruling bench_reference = proven_engine_reference (conformance/rulings.pl:538, user word "2->b?") settles BOTH phase-0 exits: (a) tsv2(proven) is the referee past the swipl 10k wall with rust graded tick-log byte-diff against it (CONTRACT.md:505), (b) final-state hash retained as a third check beyond, not a substitute (CONTRACT.md:507). 5 scale cells identical_vs_reference at 10k/100k (CONTRACT.md:624-630). Residual, published not open: a bug shared by tsv2 and rust passes the diff past the oracle budget (CONTRACT.md:555-582). Rust phase 1 grading unblocked.
```

## unsupported_messages_arc

Source: `v6/prolog/ARCH.pl:880`

```text
LANDED 2026-07-31 (merge 3ffc1074, codex luna): beta gate 1 -- 107 specific unsupported construct message clauses 0 fallback, parse errors line:col furthest-failure, text door file:line at/3; serve-path exit classifier three-shape fix by coordinator.
```

## getting_started

Source: `v6/prolog/ARCH.pl:881`

```text
LANDED 2026-07-31 (merge a5ad6233, opus): beta gate 4 -- 24-block executed doc, all replay-gated (just getting-started, green-all), persistent-shell replay so cwd/exports/background serve behave like a reader's terminal. 5 cold-author defects filed on the row below.
```

## grader_exit_gate

Source: `v6/prolog/ARCH.pl:883`

```text
LANDED 2026-07-31 (4a6a6c1c, failure-modes class 37): grader run/1 was exit-0 advisory since birth; now accumulates failures + fails the goal (every -g go runner inherits exit 1). Red float fixture (two deltas/2 terms for one rel) fixed same commit; shipped through 4 green batteries undetected.
```

## manifest_reason_diff

Source: `v6/prolog/ARCH.pl:884`

```text
PROPOSED by the fork_join lane 2026-07-31: a unsupported construct-reason-level manifest diff as a standing check -- the lane's own first draft silently rewrote 2 edge fixtures' unsupported construct reason (same bucket, same counts, sweep green) and only a manifest diff caught it. Small: sweep already writes the manifest; the check is a git-diff classifier over (name, bucket, reason-functor) triples.
```

## glob_dialect_split

Source: `v6/prolog/ARCH.pl:886`

```text
LIVE DEFECT measured 2026-07-31 by the scan-card lane: bind watch's BOOT half (git ls-files pathspec, 2_binds.ts) and LIVE half (node path.matchesGlob) disagree on 170/242 corpus globs -- src/**/*.rs boots 0 of 145 direct children, **/*.md drops all root-level files at boot then rows appear on edit and del on restart (reproducible from GETTING-STARTED's own tutorial glob), brace globs boot ZERO rows silently. Fix = ONE dialect both halves; WHICH dialect = SLOT-GLOB-DIALECT ruling, but boot==live is unconditional. HIGH -- corrupts the extraction feed for common globs.
```

## files_naming_p1

Source: `v6/prolog/ARCH.pl:888`

```text
LANDED 2026-07-31 (opus worktree, merge e3f5064f; coordinator re-ran conformance 279 + FILES HOSTS HOLD on merged tree): ruling files_naming half-executed -- enumerate/enumerate_at -> files/files_at (program-text sh decls, rename not kernel), scan = reserved removed_word unsupported construct BOTH doors naming the replacement (fail-first fixtures: was a silently empty EDB rel via edb_definition), golden coverage row, print_dl goal() role clause (first draft caught by G1). Part 2 (repo column) STOPPED at the spelling fork, correctly.
```

## files_repos_p2

Source: `v6/prolog/ARCH.pl:890`

```text
LANDED 2026-07-31 (opus worktree, 5 commits): repo_files/repo_files_at (git -C templates, unscoped pair byte-untouched) + repo_file/3 as its OWN rel (a path is only a key alongside its repo); sprefa_extract_repo executor with its own exact 3-column contract (NOT a widened sprefa_extract row) selected by a {repo}/{path} template match, batching kept via an ApplicativeExecutors set; crawl_org.dl6 = stopping-point program #5 (want_org -> repos-on-a-clock -> repo_files_at -> repo_extract -> banned_call, gh_repos written+ungraded with its clone-host gap NAMED); crawl-bench v6 leg = ONE server/db/program/arrivals-batch, the repo loop DELETED. Receipts: conformance 279 -> 281, sweep BOTH modes 196 compiled/195 identical/0 wrong (restated=0 bucket_moved=0 added=2), TEXT_DOOR 196/196/0, roundtrip ALL PASS, plunit 268, tsv2 125/1skip, FILES HOSTS HOLD, GETTING STARTED HOLDS, EXTRACTION LIVE HOLDS. CRAWL BENCH before/after, same corpus + same extraction leg: cap 8 = 20.26s -> 18.08s (1.12x), cap 32 = 72.19s -> 57.55s (1.25x), row counts identical both sides, stmts/tick flat 54.03-54.04 -- the saving is per REPOSITORY (~0.27s at 8, ~0.46s at 32), so the ratio grows with repo count. Pre-change numbers pinned in scripts/crawl-bench-loop-baseline.tsv (the loop cannot be re-run). MEASURED AND REJECTED mid-flight: capturing the extractor's whole JSONL instead of one row per file takes the 779-file corpus 20.26s -> 62.97s and the db 1.0MB -> 595MB (real extraction-seam number, wrong question for this bench).
```

## unprobed_host_no_rels

Source: `v6/prolog/ARCH.pl:891`

```text
DEFECT FOUND+FIXED 2026-07-31 by the files/repos p2 lane writing the ungraded gh_repos decl: generated_host_decls/7 is only reached from expand_probe_rules/5, so a DECLARED-BUT-UNPROBED sh host produced a host PLAN naming __host_demand_<name>/__host_response_<name> and ZERO decls for either. POST /program answered 200 and the served process then DIED on `unknown rel '__host_demand_gh_repos'` out of the boot demand scan (serve/3_engine.ts:143) -- a 200 followed by a dead server, the self-diagnosis law's exact complaint. Fix = 1_host_expand.pl:unprobed_host_decls/3, base-arity demand+response decls for any host no probe declared (the reading that matches `rel` and the edb_definition ruling), arity-guarded so a salted probe never gets a base-arity twin. plunit fail-first test unprobed_host_still_declares_its_relations. RESIDUAL, harmless and stated: an unprobed host's demand rel has no rule, so it reads as an arrival target and appears in the load response's arrivalTargets; nothing pushes it.
```

## host_arity_overload_miscompile

Source: `v6/prolog/ARCH.pl:892`

```text
CLOSED 2026-07-31 by the files/repos p2 lane: 1_host_expand.pl:no_duplicate_host_names/1 throws duplicate_host_decl(Name) at load, both doors, with the reason stated (host_relation_refs/3 derives both generated rel names from the NAME, so the name is a key). Fail-first conformance fixture duplicate_host_name_is_refused. It is the enforcement half of ruling repo_column_spelling = distinct_name_hosts -- the ruling says the repo case gets its own name, this says the language will not let it not.
```

## bind_repo_column_unsupported

Source: `v6/prolog/ARCH.pl:893`

```text
LANDED 2026-07-31: `repo` on ANY bind decl is the named unsupported construct bind_repo_column(Name) rather than the generic bind_mismatch, with four reasons written at validate_bind_decl/3 -- (1) a crawl ENUMERATES and does not react, and a bind's configuration column is read from program LITERALS (emit_ts.pl bind_read_literals/4) not from row deltas; (2) the watcher handle budget and every phase-2 flatness receipt are per working tree; (3) (glob, path, digest) stops being a key once two repos hold the same path; (4) crawl freshness is already spelled twice (rev pin caches forever, interval bucket mints a fresh witness). Decidable at load, so the day a program proves the gap the unsupported construct is where the argument reopens. Fixture repo_on_bind_watch_is_refused.
```

## dataflow_atlas

Source: `v6/prolog/ARCH.pl:894`

```text
LANDED 2026-07-31 (opus worktree, merged; coordinator ran just atlas TWICE, worktree AND merged tree, ATLAS HOLDS byte-stable both): the proof-of-worth rail -- v6/dl/fixtures/dataflow-atlas.dl6 + tools/xref_facts.pl (library(prolog_xref) JSONL) + scripts/atlas.sh + DATAFLOW-ATLAS.{dot,svg,md}. 421 nodes / 809 edges over 4 auto-extracted planes (ts 258 defs 542 calls via extract bin; prolog 1241 preds 3143 xref calls; shell 695 goal mentions; sql 29 touches), 7 named bridge rules, bop-run -> 13 sqlite tables in 12 hops (longest 19), sprefa-extract cluster, graphviz TB ratio=0.4. Sabotage: one bridge deleted = LONGEST 19->11 + chain EMPTY (node count blind -- edges are the assertion). 5 compiler unsupported constructs recorded as findings; recursive longest-path COMPILED first try (needed acyclicity, not a construct).
```

## scip_families

Source: `v6/prolog/ARCH.pl:900`

```text
LANDED 2026-07-31 (opus worktree, merge 6fc751f6; coordinator re-ran crate 58/0 + SCIP FAMILIES HOLD in worktree): sprefa-extract --family scip = real index data (v5 scip_setup/scip_import ported forward: INDEXERS rust/ts/go, ensure_index mtime cache, prost decode, 8 scip_* records + scip_index/scip_skip data rows); --family diet_scip = the heuristic resolve pass under the honest label, byte-identical pinned. Discriminating receipt: cross-file import call answered by scip (fn_edge + ref w/ def_file), ZERO rows from diet -- both languages. v6/tsv2 indexes ITSELF in 4.35s/120k rows. Timeout-gun native: 600s budget, wedged-indexer receipt asserts the GRANDCHILD dies. Residuals named: scip-python/java/clang rows not ported (one ScipSource build body each); scip_local 0 on ts (indexer inlines params); justfile recipe rides the timeout-lane close-out.
```

## timeout_gun

Source: `v6/prolog/ARCH.pl:901`

```text
LANDED 2026-07-31 (opus worktree, merge b10defaa; coordinator resolved the justfile conflict, added the scip-families recipe handoff, re-ran scip-families + green on merged tree): run-capped.sh hoisted (run_capped/capped/cap_self/capped_curl, pgroup KILL exit 124), served compile door = budgeted named compile_timeout answer + server survives, EVERY receipt script + long justfile recipe budgeted off measured walls, dot per-render, perl_alarm_orphan CLOSED. Class-38 find: a streaming POST needs its load budget SPLIT from the http budget (the mechanical curl cap broke atlas, only running it caught that). Residuals: v5-side dl-trace/verify.sh still carry the orphaning one-liner; call-site RATCHET (refuse new unwrapped invocations) = the promotion, unbuilt.
```

## ordered_aggregate_arc

Source: `v6/prolog/ARCH.pl:904`

```text
LANDED 2026-07-30 (merge a2a98d5d, codex luna high no-commit flow; agent stopped correctly at sandbox EPERM, coordinator ran the golden-flex leg + accepted the grown-golden compile-speed baseline): json_group_array/1 value axis + /2 int ordinal, group_concat/2 + /3, both doors byte-graded through the SHARED canonical json encoder; named unsupported constructs aggregate_separator_not_constant / aggregate_ordinal_not_int / aggregate_group_not_delta_local; empty group = absent row, executed minus-delta fixture, COUNT tests flat + EXPLAIN SEARCH; 11 fixtures incl all 4 sighting programs; golden-flex flexes all four forms (and its byte-diff caught a cross-type-decl divergence en route: text decl over int source = oracle 4 vs emitter "4", silent -- fixture candidate for json_pattern_expand). FOLLOW-UPS: self-map python mermaid renderer + extract-t2 rebuild can now shrink onto group_concat; ride the next touch of those rails.
```

## golden_oracle_arrival_fix

Source: `v6/prolog/ARCH.pl:906`

```text
LANDED 2026-07-30 (golden_json_seam codex lane, merged; coordinator ran the sandbox-blocked legs): 0_json_arrival.pl = ONE shared arrival-mapping module, dl6_oracle.pl -83 lines onto it, golden_oracle.pl consumes the same predicate (second copy DEAD, no third born); golden-schedules.ts canonicalizes json-column arrivals. Golden coverage 47 exercised / 13 named absences -- spread/$key/**/typed-capture/json_list(T)-round-trip all in the golden, all 6 legs HOLD incl served e2e.
```

## incremental_affinity_drop

Source: `v6/prolog/ARCH.pl:908`

```text
FIXED 2026-07-30 (merge 32916613): deltas carry the STORED value from RETURNING; matrix emitter_modes_disagree 48 -> 0, IDENTICAL 79 -> 87; fail-first fixture + coordinator adversarial dup-batch fixture (positional pairing under OR IGNORE partial batches proven set-equal to RETURNING). Residual doors_disagree 81 = decl-cast ruling class, waits on type_ruling_round.
```

## pairwise_single_tick_wrong

Source: `v6/prolog/ARCH.pl:910`

```text
CLOSED AS WORKS-AS-RULED 2026-07-31 (RCA + fixtures, no code change): NOT a one-door bug. Q1e reproduced from 89ccaccf on both doors and they are BYTE-IDENTICAL at both cadences (oracle via compile/scripts/dl6_oracle.pl, emitter via a compiled module on the tsv2 runtime): dense (10,9)+(14,9), one idle tick between changes (10,14)+(14,9). Cause is the composition of two ruled properties, not a lowering choice: a keyed replace departs the old ROW (keyed_replace_departs_the_old_row) and a departure is a NEXT-tick occurrence (departed_fires_next_tick_on_retraction, q4, TICK-MODEL grade +1), so the second body atom reads S one tick after the replace. TICK-MODEL section 2 already writes the arm as (dS)- at t JOIN S at t; this is that line at cadence 1. Two probes recorded: latest(...) around the read gives the IDENTICAL log (read-vs-trigger is not the axis), and the one same-tick candidate (bare arrival trigger + pre(...)) pairs every value with ITSELF because arrivals are absorbed before edge rules run. Pinned by fixtures pairwise_reads_state_at_the_departure_tick + pairwise_pairs_adjacent_values_when_the_source_idles; doc notes in SYNTAX.md finalize/1 and TICK-MODEL.md section 2. RESIDUAL, unowned and NOT a defect in the above: rx pairwise() has no cadence sensitivity, so a construct that holds the previous value rather than reading it back would be a real language addition, priced only if a program asks for it.
```

## type_ruling_round

Source: `v6/prolog/ARCH.pl:912`

```text
RULED 2026-07-31 morning + EXECUTED same day (opus worktree, 4 commits, merged; coordinator re-ran conformance 277 in worktree + just green exit 0 AND bench exit 0 on merged tree): arrival gate ALL declared types all positions (json/list via shared 0_json_arrival reader; wide-int scan decl-INDEPENDENT, fires on undeclared columns + inside json docs), shared head column wall head_column_type_conflict in 0_program_check.pl both doors (level + min/max agg heads; edge head unchanged per ruling), int->float SQLite affinity widening stated ONCE at 3 sites (the decl_type_conflicts_witness site refused the agent's own fixtures until widened = a door disagreement the lane caught), float->int only fractionless, json-capture RangeError -> int_out_of_range naming the statement. Matrix 422: DIVERGENT 104 -> 18 (ALL 18 = undeclared-column cells, the separate bare-column-default question), wide-int 42 cells 0 divergent (40 unsupported constructs + 2 agreed float widenings), 9 IDENTICAL deliberately became agreed unsupported constructs (same bytes by coincidence, type mix by ruling). +10 fixtures, sweep 196/195/0, TEXT_DOOR 196/196/0. Bigint TODO doors: 0_type_plane.pl wide_integer_witness/2 + tsv2 runtime/rows.ts.
```

## compile_trace

Source: `v6/prolog/ARCH.pl:913`

```text
2026-07-31 luna lane, merge dd6a4a23: always-on COMPILE-TRACE stderr line (per-phase wall ms + inferences) from compile_dl6/compile_program, one shared measurement impl in compile.pl (6_profile JSONL consumes it), compile-speed gate auto-profiles worst regression top-15 self-time, just compile-profile recipe. Root-cause receipt for compile_speed_regression came from hand-running the previously-never-run execution_profile_dl6: parse_dl mark_furthest/1 length(Suffix) per call.
```

## compile_speed_regression

Source: `v6/prolog/ARCH.pl:914`

```text
CLOSED 2026-07-31: cause was NOT mark_furthest/1's length/2 (that is the WALL cost, 59.9% of profiler self time); 88% of the INFERENCE blow-up was parse_dl.pl:line_at_suffix/2, called once per statement and walking the whole prefix three times (length + append + prefix_line_col) = O(file x statements), plus parse_input_codes_fact/1 copying the code list out of the dynamic database on every one of those calls. Fixed: line-start table built once via split_string + binary search over an arg/3 term, statement records hold their suffix length and resolve to a line only when a unsupported construct asks, furthest mark is a minimum remaining-length in a global instead of an index in a dynamic fact, and the three provably-dominated mark families (skip_to_eol per code, skip_ws per code, lit_dcg per matched code) collapse to one mark each. golden-flex parse 3,872,680 -> 261,179 inferences (+1888.8% -> +34.1%) and 923ms -> 233ms wall; unsupported construct line:col byte-identical over 771 broken inputs (25 hand cases + every checked-in .dl6 truncated and corrupted at 12 points each), pinned by plunit parse_error_positions. THE +10% BAND WAS NOT REACHABLE and the 3 regressing parse rows were re-baselined instead: mark_furthest runs 12,723 times parsing golden-flex, a Prolog call is one inference before its body runs, and the band is 19,472 inferences wide, so even a mark that did nothing at all would miss it (measured: 184,073 vs a 177,408 ceiling). door-handwritten's row was left alone -- at 960 bytes the tracker fits.
```

## get_else_wiring

Source: `v6/prolog/ARCH.pl:915`

```text
LANDED 2026-07-30 (ruling null_design = get_else_use_site_never_storage; candidate B, T? nullable columns, stayed DEAD and plans/2026-07-30-null-implementation-plan.md stayed superseded). SURFACE WORD IS `coalesce`, not `get_else`: the vocabulary law admits only rxjs/prolog/SQL words and COALESCE is SQL's name for exactly this. THE LOWERING PLAN THE ROW CARRIED WAS NOT NEEDED -- no LEFT JOIN family was written and the emitter gained nothing. 0_coalesce_expand.pl (expansion phase 45, between match and relation_edge, the ONE module both doors consult) rewrites one rule into two ordinary clauses of the same head: the read, and `not(...)` plus a `:=` of the default. Multiple head clauses, stratified negation and `:=` binds were all already shipped, so the construct inherits the incremental delta path, the negation path's retraction flip and the naive referee. N coalesce goals fan out to 2^N clauses. The EDGE arm is latest(...), not the bare atom, since a bare atom in an edge body is a trigger (same split, same reason, as 0_relation_edge_expand.pl). Seven unsupported constructs, all thrown by the shared expander so the doors cannot disagree: coalesce_no_output, coalesce_multiple_outputs, coalesce_output_not_column, coalesce_default_not_literal, coalesce_source_not_rel_atom, coalesce_not_top_level, coalesce_in_head -- the last two are what keep a coalesce from ever reaching analyze.pl, where the live row's refs_of_arg role would read the source as an ordinary join and drop the default in SILENCE. Receipts: conformance 201 -> 209/0 (incl a retraction-flip fixture where the defaulted row RETURNS after the source row leaves, and a derived-source fixture), sweep BOTH modes 141/139/0 -> 145/143/0 with zero movement in any prior bucket, TEXT_DOOR 141 -> 145/145/0, roundtrip G1 209/209, plunit 236 -> 239, GOLDEN FLEX HOLDS with coalesce exercised non-vacuously at all four cardinalities, `just green` exit 0. COUNT/PLAN receipt v6/tsv2/tests/coalesceCounts.test.ts: 33 statements/tick flat at 5/100/1000 source rows, default arm plans SEARCH n0 USING PRIMARY KEY off __frontier_repo, three sabotage receipts in the header. some/none stays the existing enum machinery and stacks with coalesce; generic Option<T> still rides monomorphization
```

## aggregate_text_unsupported

Source: `v6/prolog/ARCH.pl:916`

```text
LANDED 2026-08-02 (opus lane t1-aggtext, merge 6263a183): a numeric aggregate (sum/avg/min/max, all through compile_aggregate_number_operand) over a column whose DECLARED type is not a number = named unsupported construct aggregate_operand_not_number, shared load-time program_violation in 0_program_check.pl so both doors agree; oracle engine gains a runtime value guard (level_eval.pl) for the UNDECLARED-column residue the load check cannot see. Was error(type_error(evaluable, alpha/0)) bare-term noise. +5 tests. The lane corrected the coordinator's own brief by citing the oracle-throws-bare-terms law.
```

## refcount_rename

Source: `v6/prolog/ARCH.pl:917`

```text
EXECUTED 2026-08-02 (opus lane t2-refcount, merge 4b9b0efc): the vocabulary-law queued support->refCount identifier sweep -- 25 files across lower.pl/emit_ts.pl, sprefa-store js+rust, tsv2 runtime, byte-goldens untouched. The ledger's queued-name attribution (supportEdges/supportPlan/retractThroughSupport as "rust store") was partly false per the lane's predate evidence; the sweep covered both planes regardless.
```

## text_door_fact_seam

Source: `v6/prolog/ARCH.pl:921`

```text
FILED as finding F1 by the comment-rail opus lane 2026-08-04 (LANG.md:39 promised bodiless facts; compile_dl6/2 passed Initial=[] so `max_run(2).` in .dl6 text refused level_rule_no_positive_body -- no self-seeding .dl6 program could compile, every served/golden program seeded via schedule arrivals). LANDED same day (flash4 lane factseam, merge 1640b768): parse_dl.pl:1051 fills an empty body as `Head <- true`, so new exported dl6_seeded_form/3 partitions ground `Head <- true` clauses out of the rules list into the Initial seam the term door already consumed (boot_statements/seeded_refs/check_world_shapes untouched); threaded through compile_dl6/2, bop_check.pl and the 6_profile.pl path so all three text-door callers agree. Non-ground bodiless clauses still refuse level_rule_no_positive_body. Receipts: plunit 324 -> 327/327 (fail-first RED recorded in the lane REPORT before the fix), conformance 294/0, TEXT_DOOR 206/206 byte-identical, probe emits INSERT OR IGNORE INTO max_run VALUES(2). The lane corrected the coordinator's brief on the parsed fact shape (bare head term claimed; `Head <- true` actual). Goldens still seed via arrivals: a seed-route flip regenerates pinned tick logs, queued behind user word.
```

## regexp_builtin

Source: `v6/prolog/ARCH.pl:922`

```text
LANDED 2026-08-04 (codex luna lane rxregex, merge before 7462b380): regexp/2 positive body condition, SQL vocabulary word. Runtime = libsql's NATIVE REGEXP (probe: SELECT 'abc' REGEXP 'b' -> 1 through createClient; flavor = Rust regex crate, rejects lookahead `'?' without operand` and backrefs `unknown \ escape`); oracle = SWI library(pcre); patterns literal-only; 4 named load unsupported constructs (regexp_pattern_not_literal / _outside_subset / _invalid via re_compile / regexp_operand_not_text) shared in 0_program_check.pl so both doors agree. Conformance fixture 9_regexp.pl; text-door 209/209 at landing; plunit 331. The lane's FIRST run STOPPED correctly on a wrong coordinator brief claim (better-sqlite3 at the seam; actual = @libsql/client, engine/lib.ts) and was resumed on a regrounded brief; the "sandbox Unicode/locale gate failures" it reported were environment artifacts, all green own-run. Coordinator fixed post-landing: the operand check matched by UNIFICATION instead of identity (any int column beside the operand refused; BodyVariable == Operand now; regression test regexp_operand_beside_int_column_compiles).
```

## ast_op_wire

Source: `v6/prolog/ARCH.pl:923`

```text
LANDED 2026-08-04 (two parallel codex luna lanes, merges d88e858f + 598d5182, contract plans/2026-08-04-ast-op-contract.md): lane A `extract query` subcommand (src/0_query.rs, full tree-sitter queries, [ ] alternation, #match?/#not-match?/#eq? via rust regex, flat JSONL captures + line/end_line, exit 2 named stderr); lane B ast/4 as expansion phase 46 (0_ast_expand.pl) desugaring to minted per-(lang,query) sh hosts over $DL_EXTRACT_BIN -- ZERO new runtime, spine_residency's executor stance kept, its surface consequence reversed by user directive. v5 capture-binding law verbatim (@cap binds same-named vars, line/end_line when used). 4 unsupported constructs (ast_query_not_literal / ast_lang_unknown / ast_query_single_quote / ast_no_named_capture). Live handshake: fact-seeded probe -> 4 rust fns with lines, struct filtered by #match? ^[a-z].
```

## cst_native_syntax

Source: `v6/prolog/ARCH.pl:924`

```text
LANDED 2026-08-04 (codex luna lane cstnative, merge add78de9): unquoted `cst(path, digest, lang) { s-expr }` body item parsed by parse_dl.pl into the ts_query term vocabulary that already existed in conformance fixture native_ts_query_term (2_hosts_wiring.pl:200), serialized onto ast/4's expansion; round-trip parse(serialize(term)) pinned; unsupported constructs cst_capture_unused / cst_variable_uncaptured; #match? patterns ride the regexp subset check. The capture-unused unsupported construct caught the coordinator's own first rewrite of comment-prod.dl6 (captured @comment_text bound nothing).
```

## comment_prod_expression

Source: `v6/prolog/ARCH.pl:925`

```text
LANDED 2026-08-04 (coordinator, f76ef6f4 then 00297c93): v6/dl/fixtures/comment-prod.dl6 = the comment budget with string+syntax logic in-language (cst blocks for nodes incl rust alternation; regexp/2 for exemption globs, extension routing, prose [A-Za-z], shebang, waiver; count aggregate over materialized range join). Hosts = git plumbing + raw-lines feed verb only. Five live receipts: 3-prose exit 2, 2-prose 0, 1-prose block 0, waived 0, rust-alternation 2. Two engine defects found by writing it, both fixed with regression tests: fact seam skipped the query (program/3) parse form, and a compound-term fact wrongly entered the seed path (fact_args_atomic guard; the native_ts_query_term fixture was the witness). Stated deviation (cst's host read the worktree file) CLOSED same day by ast_query_blob_door.
```

## comment_rail_prose_count

Source: `v6/prolog/ARCH.pl:926`

```text
LANDED 2026-08-04 (flash4 lane proserail, merge fd7944ca/6c3e928c): the budget counts PROSE lines only (letter-bearing after token strip); delimiter/divider/shebang lines are glue -- contiguous, unmeasured. Host emits prose_flag+prose_seq; run_prose_row materialized range join (SEARCH-pinned) because aggregate_group_not_delta_local refuses a range join inside min/max -- the lane corrected the brief's spelling and recorded it. Parity 9 cases. Kills finding F5 (shebang counted as prose by both tools). Also this day: rail default flip 4e7a32d6 (dl6 rail = pre-commit leg, SPREFA_COMMENT_RAIL_DL6=0 = retired bash fallback).
```

## extract_md_html_query

Source: `v6/prolog/ARCH.pl:928`

```text
LANDED 2026-08-05 (flash4 lane mdquery, merge 247c6561 amended): extract query gains langs md (tree-sitter-md 0.5.3 BLOCK grammar, the exact v5 pin so block parses match the v5 oracle), md_inline (INLINE_LANGUAGE, dropped in with zero executor change), html (tree-sitter-html 0.23.2, unified with ast-grep-language's transitive copy; cargo tree receipt = ONE tree-sitter 0.25.10 core). Program whitelist 0_program_check.pl:245 takes md+html (lane) and md_inline (coordinator post-landing: a lang the CLI can query but dl6 cannot is an asymmetry against the querying-prose-from-the-language goal). Narrows the doc_format_extraction markdown hole: block+inline markdown and html are now cst-queryable; the *.md comment-rail exemption stays until a rail wants prose grading. Lane STOPPED first on a wrong coordinator brief claim (whitelist claimed in 0_ast_expand.pl; real site 0_program_check.pl:245) and resumed on amended ownership. 3 new cargo tests; plunit 345 unchanged (whitelist unpinned by tests).
```

## rel_catalog_ts_field

Source: `v6/prolog/ARCH.pl:931`

```text
LANDED 2026-08-07 (step B1 of plans/2026-08-07-dynamic-loading.md): catalog_rows/4 lifted out of catalog_row_ddl/4 so the INSERT and the emitted constant read ONE source (lower.pl:708); emit_ts program_catalog_rows/4 + rel_catalog_lines/2 render `const relCatalog: readonly IRelCatalogRow[]` on EVERY module, not only the ones naming __rel, so a reload can compare; IRelCatalogRow declared in tsv2/runtime/types.ts and relCatalog required on IServedProgram. Cost, measured, gated: emit inferences +96k golden-flex / +9.7k door-handwritten (linear in rels+columns, two sha256 + one term_to_atom per row), baseline re-pinned. Receipts: sweep RUN 211 identical=210 wrong=0, MANIFEST_REASON_DIFF restated=0, typecheck clean, tsv2 156 (155 pass 1 skip), staleness-gate OK.
```

## emit_observers_quadratic

Source: `v6/prolog/ARCH.pl:932`

```text
DEFECT FOUND+FIXED 2026-08-07 by the compile-speed ratchet reading RED on clean main HEAD, not by the branch: rel_rule_observers/3 (ce6286bd) is called once per relation entry and each of its five clauses walks EVERY rule, so emit was rels x rules. golden-flex emit 359,701 -> 1,031,323 inferences (+186.7%), flagship-flow +22.4%, and the lane that landed it never re-pinned. Fix = rel_rule_observers_map/2, one findall over the rules for the whole program, grouped by ref; rel_rule_observers/3 kept as a map lookup so its 6 plunit cases are unchanged. golden-flex emit 1,144,701 -> 467,429 with the catalog included, byte-identical output on all 211 fixtures. Residual over the old pin is the catalog feature above, not the walk.
```

## files_at_receipt_false_green

Source: `v6/prolog/ARCH.pl:933`

```text
DEFECT FOUND+FIXED 2026-08-07: files.sh step 4 waited with `await_rows file "$before"` where before = the count step 1 had just asserted, so the wait returned on the first poll and `sleep 3` was the whole budget for a host spawning one `git rev-parse` per tracked path. On a CLEAN tree the leg passed regardless, because files (working-tree hash-object) and files_at (blob oid at the rev) answer the SAME pair there; it went red the first time it ran in a tree with edits. Fix = wait for `before + edited` (git diff --name-only counts the paths whose pinned row is new) and pin the assertion to an EDITED path where one exists. docs/failure-modes.md class 42.
```

## enum_column_type_erased

Source: `v6/prolog/ARCH.pl:935`

```text
DEFECT FOUND 2026-08-08: an enum name cannot be used as a column type even though it monomorphizes into real tables. `rel grade(ripe(sugar: int) ; green(days: int)).` + `rel picked(id: int, g: grade).` refuses with column_type_unknown; so does `g: grade_tag`, whose table physically exists with __refcount and a full delta family. `rel span(start: int, end: int).` used as `at: span` compiles, so the capability splits on variant-ness while both are spelled `rel`. ROOT CAUSE is phase order: type_decl/2 is minted by the PARSER (compile/parse_dl.pl:834 normalize_relation_value_decls) from col_type entries, and enum expansion runs later (1_expansion.pl:69), so the enum name has no relation_schema at the moment types are collected and 0_type_plane.pl:62 type_definitions/2 never sees it. 0_enum_expand.pl:12-16 warns about exactly this hazard and mints enum_context/2 for it; the two consumers are 0_match_expand.pl:22 and 1_expansion.pl:69, and 0_type_plane.pl mentions enums zero times. FIX SHAPE: hand enum_context to the type plane and resolve an enum name to ref(EnumName_tag), storing one INTEGER id with the variant recovered by joining the existing tag rel. This is the oneOf carrier for emit_json_schema/emit_openapi; the flat one-row-per-enum presentation belongs at the wire boundary where type_canonical_json/4 already lives (0_type_plane.pl:11), not in storage, because a UNION ALL view needs NULL padding and would reintroduce the three-valued logic every emitted column currently avoids. Receipts and the ready-to-paste red fixtures: plans/2026-08-08-enum-column-type-defects.md.
```

## module_identity_bytes

Source: `v6/prolog/ARCH.pl:937`

```text
LANDED 2026-08-10 (opus worktree, 3 commits, MOD-2 bytes wave). THREE RIDERS. (1) MOD-8: module_hash read the BASENAME, so a/b/c.dl6 and aa/b/c.dl6 minted one identity; identity is now short_hash of the path relative to the ENTRY's directory with the extension dropped, which distinguishes equal basenames, stays stable across machines, and coincides with the bare module name for a single-file entry -- that coincidence is what keeps the TERM door (which holds no path) byte-identical to the text door. module_hash/2 was defined twice (use_resolve.pl:244 + lower.pl:753, one cut apart); collapsed onto use_resolve:short_hash/2. (2) Per-rel attribution: rel/column/storage/plane/__ref rows carried the ENTRY's module_id, so a used module's rel changed identity per importer; use_resolve emits rel_module_decl(Name, Hash) per file from that file's own decls (mounts excluded) and lower resolves parent_id, module_id and the h_id base through it. SHAPE TAKEN = bespoke decl terms, not @-annotation rows (ruling annotation_at_curry): the @ surface has no parser today (zero hits in compile/parse_dl.pl) and module attribution is compiler-derived, never author-written; re-homing onto @ rows when that surface lands is mechanical. (3) Module graph: module_edge_decl(ConsumerHash, ProducerHash, Kind, LocalName) per use -- kind use always, kind mount beside it for an alias (mount_alias_additive) -- minting catalog rows so module(id, name, hash) = the kind=module rows and module_edge(consumer, producer, kind) = parent_id/module_id/kind on kind in {use, mount}. A bare use minted NOTHING before. The parent edge enters the EDGE's h_id, never the module node's: a node hash carrying its parent mints two identities for one diamond-shared file and breaks the loaded-memo dedup, so node identity stays the resolved path. Grain = file-to-file; export-to-import reference grain needs analyze to attribute each body atom to its declaring module (own arc). RECEIPTS: conformance 346/0, plunit 553 -> 557 (MOD-8 pin flipped fixme -> passing, 4 new mount_door tests, sabotage receipt on the attribution test), TEXT_DOOR 246/246/0, roundtrip ALL PASS, and the compile sweep rewrote all 1066 tracked out/ artifacts with ZERO git churn -- the byte-identity receipt. RESIDUE: a `use` row's local_name is the producer's module name and can collide with a consumer rel of the same name under one parent_id (the mount side has mount_path_collision, the use side has no guard); a rel declared in NO file (rule-head only) still attributes to the entry.
```

## ast_query_blob_door

Source: `v6/prolog/ARCH.pl:938`

```text
LANDED 2026-08-04 (flash4 lane blobdoor, merge 99aec99e): `extract query --digest <oid>` reads the source via `git cat-file blob` (0_query.rs source_bytes/cat_blob; digest absent = worktree path read unchanged, so non-git callers keep the path form); minted cst/ast host template passes `--digest {digest}` replacing the `: {digest};` no-op (digest was already a demand column, so freshness rode the key all along -- the door makes the READ match it); comment-prod.dl6's stated worktree-read deviation paragraph deleted. Receipts: cargo 9_query_cli 5/5 (digest==path parity, bad-oid exit 2 one-line stderr), battery green, live rail run: 3-line staged finding survives an unstaged 10-line append, RESTAGING moves the finding to the new lines (coordinator re-verified the restage half; the lane's report showed only the unstaged half). Design residue: digest resolution is git-bound inside extract (a non-git digest source has no door); wrapper alternative not taken, revisit only if a second digest source appears.
```

## dd_plan_dd_runner

Source: `v6/prolog/ARCH.pl:948`

```text
LANDED 2026-08-09/10 (d6a3987a, 5f5cf3b3): 6_isolated_compiler_dd.pl (733 lines) emits the dd_plan term + JSON twin over lowered/8 (3 goldens byte-clean); v6/dd-runner main.rs (dd-diet-rust-sqlite arm) + kernel.rs (dd-diet-rust-rust arm, 215 lines, zero SQLite) consume it, 3 fixtures byte-clean. NOT in any battery. Renamed 2026-08-12: arms say what they are (diet = dd-shaped without the algebra); dd-rust-dd is a reserved slot for the real differential-dataflow crate, not built.
```

## recursive_cte_probe

Source: `v6/prolog/ARCH.pl:952`

```text
LANDED 2026-08-11 (00ad3f68): adds retract_signed_delta_v2 + retract_delta_fold + the recursive-CTE probe (examples/recursive_probe.rs, PROBE-REPORT.md). Signed survivor reachability is one WITH RECURSIVE walk plus frontier clear plus weight publish: 3 statements at DAG 960k.
```

## perf_report_refresh

Source: `v6/prolog/ARCH.pl:954`

```text
LANDED 2026-08-11 (471d0be9): PERF-REPORT refreshed with the coordinator verify run; the seven-engine matrix. DAG 960k: dd 174.6 (0 stmts, 1.00x), sqlite-signed-delta-v2 1135.6 (3, 6.50x), sqlite-dred-loop 1774.6 (53, 10.16x), sqlite-dred-cte 2582.7 (6, 14.79x).
```

## dd_runner_tick_phases

Source: `v6/prolog/ARCH.pl:955`

```text
2026-08-11: dd-runner's tick loop matched ONE of the twelve tick_order phases (6_isolated_compiler_dd.pl:729-733) and, because arm dispatch was `operators.is_empty()` and dd_plan always emits operators for a program with rules, the sqlite tick loop had never run for any graded fixture -- all 3 took kernel.rs. Arms are now --dd-diet-rust-sqlite (default) / --dd-diet-rust-rust. Nine phases execute (absorb_arrivals, level_before_edges, edge_arrivals, edge_departures, level_after_edges, iterate, boundary, carry, drain); index_delta, consolidate and retain name the lowered/8 field 6_isolated_compiler_dd.pl drops (deltastmt/5 args 3-5 at :86, rel/3's Kind at :83, retentionstmt/3 at :609) instead of no-opping; an unknown phase name is a hard error. grade.sh went from 3 hand-named fixtures in NO gate to all 200 fixtures with both a dd plan and an oracle tick log, 131 byte-clean, ratcheted by graded.<arm>.tsv in both directions and by budget.json's 8 MB peak-RSS ceiling (measured band 4784-5808 kB; RSS is graded because the TypeScript OOM that announced an unbounded row unload is silent in Rust). `just dd-grade` is green-all leg 32. mutual_recursion (6_isolated_compiler_dd.pl:468) fires on ZERO corpus fixtures.
```

## shared_frontier_lowering

Source: `v6/prolog/ARCH.pl:960`

```text
LANDED via PR #386 (b0c319e57, supersedes #378), NOT the branch feature/shared-frontier-fable, whose six commits main already carries byte-for-byte. `frontier(shared)` on compile_dl6/3 puts one __frontier / __next_frontier / __support_count behind per-rel TEMP views (lower.pl shared_frontier_view_ddl/3), plus the six write verbs (lower.pl write_verb/1, src/write_verbs.rs). Default stays per_rel and absent-option output is byte-identical: grade.sh graded=440 byte-clean=335 unmoved. shared-frontier-gate.sh 8/8 PASS, the four sf_retract_*/sf_negation_support/sf_two_rule_support arms graded against the oracle, so step 5 (retraction against the shared support table) IS built. MEASURED 2026-08-22 at c88ebb0fd, v6/labs/BENCHMARKS.md "shared frontier arms": statements per fold -21.0% / -25.2% / -26.3% at 8 / 32 / 128 rels and fold wall -6.1% / -5.3% / -14.0%, so the runtime claim holds and scales; emitted bytes +8.4% to +9.9% and corpus DDL bytes +14.8%, so the codegen-size claim the plan was written for is INVERTED.
```

## one_tick_path

Source: `v6/prolog/ARCH.pl:964`

```text
LANDED 2026-08-23 (ruling per_rel_delta_only): ordered_program/1 and ordered.rs are DELETED. Each edgestmt carries its own ArmSchedule (set_at_once/sequenced) from its kind; a sequenced arm walks its trigger's frontier in (_phase, _sequence) order inside apply_edges, every arm of one occurrence projecting before any writes, occurrence-major across arms because one_pick_order referees by arrival index; a row written then overwritten in the tick stages its NET into the carry. snapshot_pre keeps ordered.rs's phase position (after arrivals, before levels), so pre(level_head(..)) still reads last tick's settled rows. TickWork::probe opens the tick with one chunked EXISTS read and level_sources maps every table name a rel owns back to that rel, so a rel does work only when a rel it reads moved; a level reading a table no rel owns never skips. Receipts: ghcache statements/tick 447,178,224,483,249,505,367,70,143,256,165,658,264,58 (ordered+#423) -> 475,522,1172,1364,1318,1771,1200,702,691,676,668,1643,1208,199 (the two count different shapes: whole-table rebuilds vs per-level delta inserts), tick log byte-identical, settled idle tick 3 statements, unlabelled calls inside ticks 997 -> 0. conformance 444/0, plunit 1076/0, RUST-GRADE graded=444 byte-clean=340, cargo 163/0, ghcache ticks=14 pr_transition_open_merged=1, goldens=6, ARCH 7/0. Disclosed en route and fixed: an aggregate never rescoped on the rel its own body negates (docs/failure-modes.md 81), with a narrower residual open for the user.
```

## file_span_redesign

Source: `v6/prolog/ARCH.pl:764`

```text
The struct arc's free-floating `type span(start: int, end: int)` is the single hole behind four shipped warts: path riding as a sibling column beside every span; flagship-flow.dl6 hand-building identity with concat([path,':',start,':',end]) (that concat IS the missing file reference); comment rails shelling out to grep because text lives nowhere; line/col living in a python referee translator while LSP line numbers ship as 0. line(span)/col(span) derive through a per-digest newline index IN-LANGUAGE; slice(span, from, to) is sub-range projection (NOT destructuring -- the running assign lab was briefed on the wrong reading). Extractor untouched: the wire keeps bare byte ranges and the HOST BOUNDARY pairs each record span with the demand's file. 3 user decision cards open in the doc
```

## file_span_storage_lab

Source: `v6/prolog/ARCH.pl:765`

```text
SELECTED physical row: file_span(file_span_id,rev_file_id,start,end), facts carry one dense id; no blob_span table. Real-distribution replay, 405,696 facts, two fresh-process runs: selected 32.24 bytes/fact and 517.6/530.5ms ingest; two-level located/content span 38.30; content-span ref 43.89; embedded coordinates 51.95; repeated TEXT 315.31. Selected filter + reverse placement paths are covering SEARCH only. PATH VERDICT over 2996 real paths x20 refs: whole-path dictionary 954,368B / 0.0385-0.0387ms prefix search; segment+junction 1,146,880B / 0.0573-0.0588ms; repeated TEXT 3,448,832B / 1.022-1.023ms. CONTENT VERDICT over 300 Git blobs/8.50MB x3 reads: persistent git cat-file --batch 58.25-58.88ms; optional SQLite stored_blob 12.85-12.88ms and 8.67MB; 1MiB LRU held at 1,048,564B, all newline indexes 212,892B. NULL-FREE shape: committed_rev and work_rev are total variant rels with ordinary union rules; git_blob and stored_blob are ADDITIVE capability rels, so a BlobId may have either or both. HOST verdict: reuse sh_decl/probe's one demand-response plan with a registered typed non-shell executor; bind_decl remains continuous discovery; no file-specific call syntax. TWO user cards remain before implementation: generic relation-reference column type (physical dense int, static FileRef/BlobRef/FileSpanRef) and typed-host authoring spelling. Existing enum tag projection stores TEXT and must use declaration-order ordinal when materialized or be omitted when match reads variant rels directly
```

## rel_value_unification_lab

Source: `v6/prolog/ARCH.pl:766`

```text
CORRECTED MODEL: referenced rel remains an ordinary public/queryable rel; parent typed column stores an edge endpoint to the target row; one target table with hidden physical __id and content/key uniqueness; temporary __ref_<name> join view only; zero __dict_* tables and zero stored __semantic/__rendered JSON. 9-check actual compiler hole lab passes: public target, typed edge, one table, direct RHS query, indexed dereference, no FK/cascade, and confirms two holes. HOLES: existing key(...) is not yet the reference identity; old content-DAG check still rejects keyed entity cycles. Both use existing semantics and justify no new construct. Sweep: 102 compiled, 91 unchanged identical, 9 expected old nested-JSON oracle disagreements, 2 pre-existing run errors. NEXT: key-driven identity, keyed-cycle acceptance, oracle relation-row migration, delete StructPlane/dictionary naming
```

## file_span_kernel_host_boundary_lab

Source: `v6/prolog/ARCH.pl:767`

```text
REL EDGE: typed relation constructor currently emits a JSON compound into an INTEGER endpoint; opaque row identity capture is unavailable; ref/1 is unregistered and emits another JSON compound. Therefore automatic key-driven edge construction is the next implementation, while ref remains unselected pending an opaque-identity receipt. STRINGS: 3019 paths x20, dedicated whole path 962560B/15.9417B per ref/0.0374-0.0381ms; universal strings+path 1036288B/17.1628B per ref/0.1472-0.1476ms. Across 200 extracted files, 39642 name occurrences and 6269 names had zero path/name text overlap; universal and separate dictionaries both used 1728512B. HOST: existing shell executor is one spawn per witness; span text must use registered batched execution behind existing demand/response rels, grouped by blob and repo, with no new DL6 spelling. SELF-HOST: relation joins, slice arithmetic, line/column aggregates, variants, and capability selection. EXTERNAL: Git/filesystem observation, byte acquisition, optional persistence, newline scan
```

## rel_edge_clock_fixpoint

Source: `v6/prolog/ARCH.pl:768`

```text
ITERATION 3: pin target arrival, missing target, keyed replacement, retraction, dangling-edge antijoin, and exact oracle/emitter ticks. ITERATION 4: attempt opaque identity capture/transport through current variables and modes; ref remains unregistered unless a required actual-world case survives. ITERATION 5: extractor -> rev_file -> file_span -> batched span_text/newline provider vertical slice. LOCKED: rel-only declarations, public queryable target rel, ordinary target table with hidden dense id, integer parent endpoint, key defines semantic entity identity, no dictionary/stored JSON/NULL payload/cascade, demand-response provider grouped by repo/blob
```

## key_edge_case_census

Source: `v6/prolog/ARCH.pl:769`

```text
HOLES: key(0), key(arity+1), and duplicate positions survive planning; keyed self/mutual entity cycles inherit the content-DAG unsupported construct; construction still consumes full target arity and emits JSON. CLOCK EDGE: positive keyed arrival replaces by key while negative arrival deletes the exact full row, so stale retraction does not remove the replacement. GATES before reference implementation: validate key positions, pin replacement/retraction ticks, and pin conflicting non-key fields for one key
```

## reference_construction_contexts

Source: `v6/prolog/ARCH.pl:772`

```text
Missing target constructor performs no existence join. Runtime INSERT OR IGNORE is key-constrained but lookup uses the full row, so same-key/non-key conflict finds no id. Key-only arity is currently just a compound term. Boot path queries removed __semantic columns absent from DDL. LEADING MINIMAL LOWERING: in derived rules, relation-shaped value is an indexed match against an existing public target row and projects its __id; missing target means no parent derivation; creating the target is an ordinary target-headed rule. World arrivals remain the boundary case: atomically resolve/assert target before parent and refuse same-key conflicts by name
```

## existing_target_identity_prototype

Source: `v6/prolog/ARCH.pl:773`

```text
COST: one compile-time term binding, zero JSON/subquery/extra SQL/hidden write/surface syntax. Receipts: construction contexts 8 PASS, plunit 147 PASS, sweep 164 total/103 compiled/61 unsupported/0 crash, runtime 92 identical/9 expected old relation-value oracle disagreements/2 recorded run errors. OPEN: direct edge trigger still lacks __id binding; constructor without explicit target atom still emits JSON; boot/world arrival still uses obsolete full-row interner and __semantic path
```

## clock_checker_proof_payoff

Source: `v6/prolog/ARCH.pl:776`

```text
EXECUTION-ONLY: general tick placement, glitch behavior, keyed batch order, host response timing, oracle/emitter equality. MISSING: registry ring/grade metadata, inferred clock expressions, labelled-SCC causality/productivity, external provider liveness theorem, boundary referential integrity. RANKED ZERO-SURFACE ITERATION: (1) project rule dependencies labelled ring/sign/grade from current AST and registry; (2) infer path offsets and reject unequal clocks; (3) accept only monotone-B zero-grade SCCs and positive-delay recurrence; (4) compile live-parent/missing-target as boundary antijoin; (5) expose proof facts for fixture comparison. Rust borrow checking supplies only the boundary-lifetime analogy because relation IDs are durable graph edges, not memory borrows. Lustre supplies clock compatibility/delay/initialization comparisons. Esterel supplies constructive same-instant SCC comparison. Four decision cards, each <=5 options; no new syntax selected
```

## flow_parity_residue

Source: `v6/prolog/ARCH.pl:782`

```text
Referee normalizes root:: symbol keys, method owner prefixes, and root-qualified types; flow_param_type 35/40 matched. Direct flow remains 2457/2457 exact. Total flow_edge: V5 2772, V6 3114, matched 2654, V5-only 118 (down from current baseline 315). BLOCKER is Rust call-target resolution evidence: targets V5 200/V6 168, matched 113, V5-only 87, V6-only 55. Current resolver picks first same-file/unique-blob definition while V5 retains qualified method targets and omits some contexts. Close with improved typed Rust resolution facts or a pinned SCIP index, not another DL rule. Classifier now fails on direct drift or empty/nonmatching node types. ADVANCED 2026-08-16 by the extractor's caller-name fact, plans/2026-08-16-flow-parity-rust-targets.REPORT.md: a closure def carries no name, resolve_at types caller_name text, so every resolved edge whose call sits inside a closure body was dropped at the typed boundary (measured: 215 resolved edges, 166 both-named = exactly the graded call_target count). project.rs names a nameless caller closure@<byte_start>. Rig twice, identical: targets V5 203/V6 193, matched 113 -> 134, V5-only 90 -> 69, V6-only 53 -> 59; flow_edge matched 2601 -> 2634, V5-only 125 -> 92; flow_param_type 35/39 -> 39/39 V5-only 0; flow_node_type 35/39 -> 39/39 V5-only 0; arg 90 -> 102, ret 126 -> 147; direct 2387/2387 exact both runs (2457 was the older corpus). RESIDUE IS ONE CLASS, all 69 rows: V5 attaches one resolved callee to EVERY call_res node of a call chain (`.entry(output.strings.lookup(name).to_string())` scores lookup at col 25 AND col 59) while V6 scores it once at the call's own position; V6 already carries the same (callee_path, callee_name) on each of those lines. Matching them means emitting V5's duplicates, not better resolution. SCIP FORK PRICED AND REJECTED: `rust-analyzer scip .` on the pinned corpus dies in 0.14s with "no projects" (13 files copied with no Cargo.toml), so option (b) needs a synthesized crate manifest in the rig plus a toolchain pin, and buys nothing this class needs.
```

## text_expression_parity

Source: `v6/prolog/ARCH.pl:785`

```text
Census already measured 57 V5 text-operation sites across =~, replace_re, trim, split, json, match_line, and match_ast; V6 registry currently has zero writable text operations. Each admitted operation lowers to SQLite when the loaded SQLite capability has an exact semantics receipt; otherwise it uses the existing typed host demand/response boundary over batched deltas. File byte slicing stays with file_span_redesign's blob/newline provider, not SQLite character substr. Exit receipts: motivating program moves from named unsupported construct to oracle-identical, SQL path has generated-SQL/EXPLAIN evidence, host path has batching and clock receipts, zero speculative string-method surface.
```

## extraction_host_batching_lab

Source: `v6/prolog/ARCH.pl:795`

```text
Release sprefa-extract proves --family df,call,type equals three separate runs. Smallest internal route groups same-tick fixed extractor plans by executor/template/path/digest/ordered inputs, runs once, projects heterogeneous stdout through existing typed output schemas, and settles existing witnesses separately. Ordinary response rel rows only; no syntax, JSON storage, or second type model.
```

## receipt_folding

Source: `v6/prolog/ARCH.pl:802`

```text
A canonical-plan or labbed status is temporary: preserve the receipt as regression evidence, attach its production consumer or decision card in ARCH, and remove duplicate lab-only semantics. Current fold order: phase5_bool_float, clock_checker_proof_payoff, rel_definition_hash_lab, file_span_redesign, comment_rail_wiring, mode/scope proof views, flow_parity_residue. Scan surface and optional JSON policy stop only at their indexed decision cards.
```

## scan_match_reconciliation

Source: `v6/prolog/ARCH.pl:804`

```text
Scan+match expands to ordinary guarded ordered edge rules: 2 persistent rel tables, 7 TEMP support tables including __pre_machine, 2 arms. Init order and T boundary/T+1 listener clock pinned. No exact-one construct exists; current 0/1/differing-N/equal-N behaviors are silent/write/keyed_conflict/dedupe.
```

## scan_surface_composition_lab

Source: `v6/prolog/ARCH.pl:805`

```text
Subscriber identity stays outside reducer key/body, so two subscribers create two views and exactly one reduction. Demand removal retracts views at T, finalize cancellation arrives T+1, later events gate; inactive cell remains because edge heads cannot delete, and re-demand resets it. Nested match reads named scan_view. Exact lowering: 10 rules, 7 edge statements, 3 level groups, 9 persistent tables, 32 TEMP, one keyed __pre_scan_cell; rule count fixed for 1 or 1000 demands. Three unselected surface cards remain.
```

## json_interop_lab

Source: `v6/prolog/ARCH.pl:807`

```text
Locked boundary: JSON/json1 is transient wire adaptation; queryable objects and arrays normalize to ordinary target rel rows plus integer reference edges and generated joins. No JSON dictionary, nested-blob storage, relation-like intermediate type, or second checker. 12/12 receipts PASS. SQLite storage receipt: 10,000 repeated objects consume 921,600B inline JSON versus 98,304B relational references, 9.38x. Five bounded user cards remain for module residency, array relation shape, null mapping, schema contribution, and recursive entity identity.
```

## byte_span_flattener

Source: `v6/prolog/ARCH.pl:808`

```text
The comment lab's flattener (host template lifts "span":{start,end} to flat line/col, since decodeObjectItems projects TOP-LEVEL declared columns only) was the cheapest fix for diag-rail.dl6's whole-file zeros and flagship-callgraph.dl6's dropped line column. file_span_redesign does the same job STRUCTURALLY -- line/col derive in-language from a per-digest newline index -- so the template hack would be work aimed at a wart that is being removed. Recoverable if the redesign stalls: 9b5ba958:v6/prolog/labs/comment_node/cn.py
```

## doc_format_extraction

Source: `v6/prolog/ARCH.pl:809`

```text
MARKDOWN is the one item with an existing receipt behind it: the comment lab named it the single extractor hole in comment parity (v5 has walk_md_comments, the cst family has no md grammar) and scoped SLOT-EXTRACTOR-WAIVER to exactly it. Four named slots incl SLOT-KEYPATH-SPELLING. Feeds schema_import_epic's json/yaml halves
```

## simplify_wave

Source: `v6/prolog/ARCH.pl:810`

```text
(2) 0_unsupported_messages.pl's umbrella renders unsupported_construct/1 only, so 1_host_expand.pl's 15 bare throws still print "Unknown message" (the B4 complaint, half-closed); (3) watch and enumerate write two DIFFERENT hash functions into one column named `digest` (2_binds.ts sha256 vs git hash-object) and nothing asserts they agree. Item 1 of the altitude set already landed at 6522f848
```

## analysis_oracle_exam

Source: `v6/prolog/ARCH.pl:812`

```text
Prior art already written: plans/2026-07-25-analysis-engine-bakeoff-labs.md holds the constraints (parse-only, no builds -- which disqualifies CodeQL's compiled-language extractors by rule; native-speed lens; a declared RAM budget per tier; same corpus, same question battery, answers diffed against an oracle) and a fixed Q battery. What has changed since that doc: we now HAVE three graded v5-vs-v6 rigs (callgraph, flow, comment) whose shape a third-engine leg can reuse instead of inventing
```

## amplification_sensors

Source: `v6/prolog/ARCH.pl:813`

```text
The gap is that NO bench in this repo SENSES amplification: `just memory-soak` asserts page-count flatness under churn and GET /stats reports page_count/freelist/dbstat sums, but nothing reports db-bytes/corpus-bytes or boundary-rows/input-row, so a 3x storage regression lands green. Sensor columns belong in the SHARED bench CSV. Diet arc only if the sensor says so -- and file_span_redesign removes the biggest class (raw path text per row) structurally, so build the sensor first or the redesign will be credited with a number nobody measured
```

## golden_flex_residue

Source: `v6/prolog/ARCH.pl:827`

```text
, while struct type names DO work there); dl6_oracle schedule_value/2 term_to_atoms a dict so struct-typed arrival columns are ungradeable through the text door; .dl6 text has no final-state oracle leg (print_ticklog/3 discards FinalAll); one ITickOutcome renders a struct column two ways (outcome.line canonicalizes per the ruling, outcome.deltas and GET /idb hand back raw stored text in arrival key order); IRowValue cannot type a struct value the engine both accepts and prints. Also banked: 14 fixtures are strict subsets of the golden = retirement candidates, and one unexplained `swipl exit null` run recorded in the rig header, not reproduced in 5 later runs (opus, lane/golden-e2e): USER DIRECTIVE -- one golden-flex.dl6 exercising EVERY live registry construct, registry-driven coverage gate (new construct fails golden until exercised = the language-kept-in-check mechanism), 0/1/many + perturbed cardinality grading, served-HTTP e2e leg; owns the serve-exit /idb race + hardcoded-port collision fixes. Motivated by the unit-maxing diagnosis: every bug this session was a composition bug under all-green single-construct suites
```

## null_coherence_lab

Source: `v6/prolog/ARCH.pl:834`

```text
First receipt decides much of it: json_extract returns SQL NULL for BOTH an absent key and a json null, which is the silent-wrong-answer class this repo keeps filing. Also: SQLite allows MULTIPLE NULLs in a UNIQUE index against our PK-over-key-columns keyed replace; DISTINCT treats NULLs equal while = does not, which lands in delta computation; prolog has no null so the oracle needs a representation that survives the sign decomposition. Prior art is unanimously against (Souffle none, classical datalog absence, Datomic absence, Flix Option), so the useful output is the SMALLEST coherent null. Tier-0 verdict required; if null is tier 0 it is the first this project accepts
```

## extract_spelunk

Source: `v6/prolog/ARCH.pl:836`

```text
Also: bin-vs-lib capability parity (nothing asserts it today), whether the missing TS module resolver is really the madge blocker given scip-typescript is already invoked by the ratchets, and the document-sources gap (no md/html/xml/toml/yaml Source, BlobSource unimplemented)
```

## ordered_aggregate_lab

Source: `v6/prolog/ARCH.pl:868`

```text
BOTH order axes proven on @libsql 3.45.1 -- value-sort = v5 parity (v5 sorts by the VALUE; no group_concat exists in v5), explicit ordinal = json_group_array(value, ordinal) two-arg; group_concat(value, sep ORDER BY ordinal) live = string-join own aggregate; incremental tier = group-scoped delete+rebuild via __agg_scope seed, SEARCH receipts, 2 statements flat 10 vs 1000 groups; empty group = absent head row; nesting composes through json(payload). Q6 minus-delta is reasoned prose only -- the wiring arc owes the executed fixture.
```

## cold_author_defects

Source: `v6/prolog/ARCH.pl:882`

```text
D5 single-output sh host truncates at first whitespace (parseWhitespace fields[0] fallback, silent). D3+D2 = the natural next errors lane; D1 = registry design card; D5 = decode-seam guard. D3 CLOSED 2026-07-31: throw_text_door_error/2 is exported from compile.pl and bop_check.pl's own compile catch site calls it, so one wrapper now serves both text-door callers; `bop check` prints `<file>:4: unsupported_construct: ...` exit 2 where it printed `rule-index unavailable`, located test + second sabotage receipt in bopCheck.test.ts and the bop_check.pl header, GETTING-STARTED section 5 rewritten (the asymmetry paragraph it documented is gone) and getting-started.sh's normalizer taught the macOS /var vs /private/var realpath spelling that a resolving CLI prints. NOTE for the wider location work (finish-the-job 9.12, review-B4): "parse_dl keeps no source positions" is now too strong -- parse_dl.pl records source_statement_fact/3 per statement and parse_dl_line_for_reason/2 finds a line whenever the unsupported construct reason mentions a Name/Arity that a recorded statement carries; what is missing is per-GOAL positions, so a reason naming no relation still falls back to rule-index unavailable. D1/D2/D4/D5 remain open.
```

## clock_check_path_blowup

Source: `v6/prolog/ARCH.pl:897`

```text
3_clock_check.pl clock_path_conflict enumerates EVERY simple path between every ref pair -- exponential in mid-chain route count. Measured: the atlas program's folds added 4 routes and compile went 30s -> 4m16s -> 9m40s at 8GB, then Stack limit exceeded inside clock_violation/2's setof at the served compiler's 1GB INSTEAD OF REFUSING (self-diagnosis law: cliffs must be named, not fatal). Fix = offset algebra per SCC/edge (the checker's own ranked item 2), never path enumeration; plus a resource-bounded unsupported construct. Also the likely suspect behind compile_speed_regression (same era, parse+check conflated in that measurement -- verify when bisecting).
```

## gen_templating_card

Source: `v6/prolog/ARCH.pl:902`

```text
(1) quote-escape = doubling rule, whole gap is ONE throw at lower.pl:221, all three spellings already parse clean; (2) regen staleness gate MISSING (self-map.sh:23 promises one, self-map/devlog in neither green recipe, ARCH-MAP.md is a release gate, 5 sightings); (3) sink shape (self-map = $document env, devlog = {document} argv at 105KB vs ARG_MAX 1MB = 10% fuse); (4) template spelling = sqlite printf (three {col} policies already in-tree); (5) NAMING executes gen_word_banned: printf > format > write (printf/format same sqlite fn since 3.38; concat ABSENT on linked sqlite 3.43.2 -- v6's concat is already a borrowed word the engine cannot honour; format( 740x in prolog impl, printf 29x as shell text in fixtures).
```

## design_archaeology

Source: `v6/prolog/ARCH.pl:903`

```text
5 only-now forms (byte-graded oracle, named unsupported constructs, endurance law, observable emitted SQL, executed docs). 6 WHAT-WAS-LOST shelf candidates: general gen construct (rides gen_templating_card), multi-repo/rev extraction plane (rides org_fanout ruling), universal first-class cursor, v4 durable mounted sql`` construct, v3 effect journal/approval saga (pending/approved/rolled_back states -- current abort ruling is best-effort only), programmable LSP-op family (dl6 has fixed boundary services).
```

## json_pattern_expand

Source: `v6/prolog/ARCH.pl:905`

```text
Fix = shared expansion 0_json_pattern_expand.pl rewriting a brace arg at a json column into atom(V), decode(V, {...}) + one 1_expansion.pl row. Adjacent named divergence to price in the same arc: untyped json capture on a NUMBER (oracle 4 vs emitter "4"; closing it = giving untyped captures a type, the widening typed captures exist instead of).
```

## type_matrix_lab

Source: `v6/prolog/ARCH.pl:907`

```text
Slot recs: decl conflict = widen edge_head_column_type_mismatch to all positions (2 vs 155 divergences, the loud position is the working one); float/int boundary already collapsed, contract owes the sentence; undeclared text default fine, its two paths must agree. AWAITING USER: unsupported construct-widening blast radius, int_widens_to_float, wide_int_fate, bool_storage.
```

## bounded_log_arm_order

Source: `v6/prolog/ARCH.pl:919`

```text
Two or more edge arms on a log head carrying keep(count(N)): retention prunes at tick END across every write in the tick, so the surviving row is whichever arm ran last, and arm order IS source line order. Swapping two adjacent rule lines changes the final state, silently, and BOTH doors agree on it so the byte-compare gate stays green. Cause is one line: analyze.pl:1331 check_no_edge_head_conflict_risk inspects KEYED heads only, so a log head is never pairwise-compared; the keyed twin throws keyed_conflict/3 (engine.pl:397) and an unkeyed set throws edge_into_unkeyed_set/1 (engine.pl:415). FIRST unsupported construct class that is INTRA-plane: all five shipped unsupported constructs (TICK-MODEL.md section 5) are cross-plane placements, which is why the clock checker was the wrong home. Two flash lanes measured the blast radius: ZERO tracked programs carry the shape (20 conformance fixtures + ~30 dl_view/labs programs have 2+ arms on one head; every log one is keep(all)), so the unsupported construct breaks nothing and had no red test until this arc authored one. Ruled REFUSE by the user 2026-08-03 over the documented-contract alternative, because everywhere else in the language moving a rule up or down is safe, and the wanted semantics already has a loud spelling in the keyed rel. Broader trigger than its keyed twin ON PURPOSE: no shared-trigger condition, because retention is per-tick-end rather than per-occurrence, so two arms on DIFFERENT triggers still collide.
```

## watch_bind_hazards

Source: `v6/prolog/ARCH.pl:920`

```text
THE DEAF WATCHER WAS NEVER A DEFECT: bop run self-terminates after BOP_RUN_IDLE_MS=2000 idle (bop.ts:165) and the coordinator's receipt scripts (3s poll sleeps) were talking to a dead process -- filed as an engine defect twice, disproved by this lane. Surviving real finding: cold-boot host spawn ~1s/subprocess (55s cold vs 6s warm for 44 grep hosts). bop-run-idle vs rail receipts (serve, or a --forever flag) = user ruling pending.
```

## catalog_g1_producer

Source: `v6/prolog/ARCH.pl:929`

```text
Landed as e3997cec: catalog_ddl_contract/2 + two []-returning stubs + the wired call site in lower_program/2 (lower.pl:3503). Shape = ONE table __catalog_rel(rel_id, parent_id, ordinal, local_name, kind, type_id), kind in {primitive, rel, column}; a column is a CHILD row of its rel, so it carries a type + annotation exactly as a rel can. Bill = 3 DDL statements per catalog-using program (CREATE TABLE, CREATE INDEX, one INSERT OR IGNORE carrying every row); across the 212 emitted modules catalog rows run 7/12/225 min/median/max incl the five primitives, and the seed adds 8.4%/14.6%/29.4% to the module's ddl array, which itself runs 714/2578/80198 bytes. Gated on program_uses_catalog/2 mirroring program_uses_tick/2 (analyze.pl:180); all 212 emitted modules stay byte-identical.
```

## enum_nullary_variant_empty_pk

Source: `v6/prolog/ARCH.pl:934`

```text
SQLITE_ERROR: near ")": syntax error`. Every fixture in conformance/fixtures/0_enum_variants.pl gives every variant at least one field, so the nullary arm has never run. Two candidate fixes: key a fieldless variant on its id column, or refuse the construct with a named message. Blocks enum_column_type_erased below, because `none()` is the arm that makes an enum an Option.
```

## derived_rel_as_reference_target_duplicates

Source: `v6/prolog/ARCH.pl:936`

```text
The target then takes rows from BOTH its own rules and the nested ingress, and the oracle returns a duplicated row: got [grade_tag(401,ripe),grade_tag(401,ripe)] want [grade_tag(401,ripe)]. Nothing refuses this. Measured the same day: zero unsupported constructs in analyze.pl or 0_program_check.pl name source or derived, and conformance/engine.pl has no bail site. CLAUDE.md carried "engine bails" from c4869c17 (2026-06-13), which described V5's rebuild_derived doing a full DELETE FROM rel; rebuild_derived exists only in v5 tests/it/*.rs and the 2026-08-02 turbo-minimize dropped that reason, so the line read as a v6 law. CLAUDE.md now scopes it to v5. Wanted: a named unsupported construct when a column type names a rel that any rule heads, plus a conformance fixture asserting it. Workaround in place: an enum column carries the instance id as int (0_enum_expand.pl retarget_enum_column_type/3), so no reference target is created.
```

## emit_rust_sqlite

Source: `v6/prolog/ARCH.pl:956`

```text
Same emit_program/5 signature so the parameterized seam at compile.pl:438 takes it with no call-site special case. Graded by tick-log byte-diff against the oracle jsonl, widened by construct class. lower.pl:5325 already keeps delta-SQL generation backend-neutral for exactly this; ruling boop_dl6_sh_door (rulings.pl:678) named rust emitters as the later arc.
```

## shared_frontier_view_inflation

Source: `v6/prolog/ARCH.pl:961`

```text
Cause is a design choice at lower.pl shared_frontier_view_ddl/3: every per-rel frontier NAME survives as a TEMP view carrying the payload column list and the __id join, so compiled reads keep their text unchanged. Three objects per rel become two (tables -22.5%, indexes -12.3%) while views go +154.9%. plans/2026-08-19-shared-sqlite-frontier.md priced pokeapi at 1,682,616 -> 716,125 DDL bytes; that number came from a rig with NO views and is not what the compiler emits. The alternative is rewriting every compiled frontier read to hit the shared table with a relation_id predicate instead of naming a view; nobody has priced it.
```

## shared_frontier_guard_lift

Source: `v6/prolog/ARCH.pl:962`

```text
Each of the eight is a TODO written without a probe; none has been tested against the standing law that a stop is a hypothesis. The one structural constraint visible in the code is that shared_frontier_view_ddl/3 joins __frontier.row_id to the durable __id, so any frontier row without a live durable row (departure) or any rel whose storage carries no __id (non_set_rel) needs an answer before its guard lifts.
```

## direct_trigger_identity_prototype

Source: `v6/prolog/ARCH.pl:774`

```text
NEXT separate hypothesis: automatically inject a target membership match when a relation-shaped head value lacks an explicit target atom; must grade recursive level fixpoint timing before landing. World/boot arrivals remain separate
```

## bigint_seam_normalize

Source: `v6/prolog/ARCH.pl:874`

```text
Also: matrix classifier does not recognize runtime named unsupported constructs (name_mismatch bucket 5).
```
