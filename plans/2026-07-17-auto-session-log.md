# Auto-session log — 2026-07-17

Persistent progress doc for the driving-window autonomous session. The task
list (R1-R8) is the queue; this file is the narrative record you read later.

**How to read this doc:** start with the caveman story right below — it is
written to be read cold, no context, each finding explained gently with the
why before the what. The tables and History further down are the terse
bookkeeping version of the same events.

## The story, caveman version (newest at bottom)

### before this session
- install new dl binary = laptop beachball. every time. for weeks.
- 6 stacked bugs. deepest one: extractor rolled dice.
- same files in, different rows out. engine believed it. rebuilt the world.
- fixed. receipt: 4.7GB per boot -> 111MB.
- war story: docs/rca-exe-swap-write-storm.md

### finding 1: determinism rail
- test: tiny fake repo. extract twice. rows must match byte for byte.
- trap I checked: if "force 2nd extraction" silently no-ops, test compares
  run to itself. always green. smoke detector, no battery.
- traced the delete keys. they are real. rail is live.
- commit a45c34d9

### finding 2: WHY instant froze your terminal
1. one-shot dl = thread cap only. no disk throttle. no low priority.
   (2 threads on 4 cores = your exact 50% cpu)
2. code comment literally admits: disk throttle is the only thing
   between a cold rebuild and a beachball. one-shots never got it.
3. bonus trap: dl asks busy daemon for work, reply wait has NO timeout.
   hang looks identical to a freeze.
4. also: ad-hoc runs scan your global config repos ON TOP of cwd.
   "index 203 files" becomes "also re-chew sprefa".

### finding 3: architecture measures, round 1 (sprefa)
| measure | top-10 says | verdict material |
|---|---|---|
| fan-out | tick/dispatch orchestrators | correct answer |
| fan-in | TypeArena.get, Parser.next, Db.conn | correct answer |
| blast radius | same names as fan-in, bigger numbers | maybe kill? |
| cycles | ONE cluster: typegraph.rs visitor | real recursion, bless it |

- 78k rows, +131KB, 4.7s. no explosion.
- full tables: scratchpad r6-measures-round1.md

### finding 4: new footgun found
- query file with no scan rule + db copy = engine ERASES the copy's file tables
- "program scans nothing" read as "no files exist". reconciled to zero.
- on a real db that is data loss.
- guard queued as task #9. (later: fixed, c3c587c9)

### finding 5: storm rails in, proven against history
- 4 tripwires committed. your requirement honored:
- oracle script checks out the ACTUAL pre-fix commits. each rail must fire
  there. each rail must be quiet on the fixed sites at HEAD. all PASS.
- found + closed one oracle hole (empty capture = vacuous pass).
- warning tier only. your commits never block.
- audit lists on HEAD: 48 / 25 / 40 / 1 findings, left for triage.
- commit 792cc902

### finding 6: orphaned effects, REAL root cause
- was never a timing race.
- templates stored as interned integer ids. loader asked for text. got None.
- so: every dynamic effect template was invisible. every boot: "no template,
  park it."
- fix: read the text view. plus re-check parked effects each cycle.
- boot receipt: 6/6 done, 0 orphaned (was 5 orphaned EVERY boot).
- commit 67ed59fe

### finding 7: instant fix, live receipt
- ran plain dl from ~/projects/instant, your exact invocation:
  - line 1: budget caps confirmed on
  - every 10s: "waiting on daemon (40s): derived call_target"
  - result at 71s. that was the WORST case (cold boot, new binary, new root)
- your terminal process: 0.03s cpu, 10MB. all heavy work in background daemon.
- if anything wedges: self-kill at 5 min, prints what it was doing.
  DL_MAX_WALL_SECS to tune.
- commit e7d29829

### finding 8: crash-tested the daemon on purpose
- triggered big rebuild. waited for heaviest phase. kill -9. restarted.
- old disease: every kill armed the next boot to redo everything forever.
- now: finished interrupted work once. settled. next cycles under 1 second.
- caveat, honest: "crash costs one component" not provable live from this
  trigger. stays pinned by test suite.
- bonus bug found: per-project perf logs all dumped in one unlabeled file.
  filed #10. (later: fixed, c33ffc04)

### finding 9: measures round 2 (smashy, TypeScript)
- same .dl file, zero rule edits. portability: confirmed.
- lesson 1: fixed thresholds do not travel. smashy max fan-out = 10,
  sprefa cutoff was 40. every "hot" list came back empty. need top-K.
- lesson 2: TS call graph 4x sparser than rust. known extraction gap.
  TS conclusions undercount until fixed.
- verdicts + data: docs/arch-measures-review.md, commit 1c3ff4a6

### finding 10: engine now keeps receipts on itself
- new: per-tick ledger. every relation that wrote rows, recorded.
- next "what wrote gigabytes": ONE sql query.
- excluded from settle check (cannot cause the bug it debugs).
- caught in review: kimi bumped schema version. would have force-rebuilt
  every project you serve, from scratch, at next boot. reverted.
- commit 4e87840f

## Priorities (user-set)

1. **R8 (top): fix one-shot `dl` choke from ~/projects/instant.** ~50% cpu,
   terminal unresponsive, user force-quit. Needs CPU budget + timeouts on the
   non-daemon path. Standing law: nothing seizes the machine.
2. R1: ban rails (railA determinism it-test, railB syntax rails + prev-rev
   oracle) — collect, review, commit.
3. R6: architectural measure prototypes (allowed to continue in parallel since
   everything is delegated).
4. R2/R3/R4/R7 queued behind those.

## Who does what, when, why

- **Fable (this session)**: coordinator. Diagnoses, writes specs, reviews all
  worker output, commits. Never delegates review.
- **kimi (`kimi -p`, background shell)**: code writer for well-specified
  mechanical tasks (tests, lint rails, scripts). Cheap, fast, needs a written
  spec and a review pass — it has produced design flaws before (the
  failed-effects retry loop) so nothing it writes lands unreviewed.
- **sonnet Explore agents**: read-only fan-out searches. Used when the answer
  requires sweeping many files (e.g. "map every CLI entry path"), so the
  findings come back condensed instead of burning coordinator context.
- **sonnet general agents**: bounded code+run tasks away from src/ (the R6
  .dl prototype file + scratch db runs).
- **opus agents**: reserved for src/ Rust changes with design risk (the
  crash-window bracket earlier). The instant fix likely goes here or to kimi
  depending on how surgical the diagnosis says it is.

Trigger order: diagnosis (Explore) -> spec (Fable) -> implement (kimi/opus)
-> review + commit (Fable). Parallel whenever tasks touch disjoint files.

## Worker roster

| worker | task | state |
|---|---|---|
| kimi (bg bash) | railB: .dl/unordered-select, dishonest-flag, lossy-dedup, static-n1 + scripts/rails-oracle.sh proving prev-rev anchors | running |
| sonnet Explore | instant-choke diagnosis | done — see R8 diagnosis section |
| sonnet general | R6: rewrite blast/cycles with key()/merge(MinBy(d)) lattice, run on scratch db copy, report | done — scratchpad/r6-measures-round1.md |
| opus (worktree) | R8 fix: apply_process_budget on every CLI entry, wall-clock watchdog (exit 124, states phase/root), attach-client read timeout (exit 75, points at `dl daemon why`) | running |

Coordinator (Fable) does diagnosis, specs, review, commits. kimi + agents do
code.

## R8 diagnosis (Explore agent, confirmed with file:line)

- Entry map: bare `dl`/single-file attaches or autostarts the daemon
  (src/lib.rs:165 gate, daemon.rs:2094 ensure_daemon) BUT falls back to
  in-process on attach failure (lib.rs:175-179). Multi-file, inline, folder,
  `--no-daemon`, `--settle`, `--watch`, `--changed`, `--verify`, and
  `dl what/q/summary` (daemon down) are ALWAYS in-process.
- In-process runs get only the 2-thread rayon cap (src/engine/mod.rs:265-278).
  No QoS, no nice, no IOPOL_THROTTLE. daemon.rs:1037-1039 comment states the
  IO throttle is the only thing preventing bulk-write beachball.
- Ambient amplification: run_file_inproc does `set_repos(load_repos())`
  (lib.rs:237-239) — config repos are scanned ON TOP of cwd root. Instant's
  own tree is small (203 git files; node_modules gitignored/pruned); the old
  instant/.dl/perf.jsonl shows 1060 files parsed across then-configured repos
  in one in-process tick.
- No wall-clock deadline on one-shot runs. `--check --max-wall` is advisory
  only (check_deadline.rs:19-20 leaves the work thread churning).
- Client attach socket read has NO timeout (daemon.rs:2080-2088): a client
  attaching while the daemon is mid-cold-tick blocks indefinitely — a silent
  hang the user cannot distinguish from a freeze.
- 2-thread cap == the observed ~50% cpu on 4 cores. The choke is disk I/O
  saturation + possibly the timeout-less attach hang, not CPU count.
- Instant is NOT in the singleton daemon's roots.json; instant/.dl contains
  a stale per-root-era daemon.log (Jul 4) and a cache.db cold-rebuild target.

Fix legs (spec handed to opus): (1) extract budget guts into a process-budget
fn called on every non-daemon entry; (2) hard wall-clock watchdog on one-shot
in-process runs that exits loudly stating root/repos/phase/elapsed; (3)
set_read_timeout on the attach client with a daemon-busy status line instead
of an infinite silent wait.

## History

### 16:0x — session resumed post-compaction
- railA (kimi) finished: `extraction_is_deterministic_across_identical_rebuilds`
  in tests/it/extraction_determinism.rs; multi-repo fixture incl. the lossy
  df_node file:line:col collision; full suite 835 passed.
- Reviewed it: verified the `DELETE FROM _reldigest WHERE rel LIKE 'extract:%'`
  wipe hits the real skip keys (`extract:<stamp>:<family>:<rev>`,
  src/engine/extract/mod.rs:82), so the second tick genuinely re-extracts and
  the test is not vacuous. **Committed a45c34d9** (scoped to the two test
  files; railB still writing in the same tree).
- Launched Explore agent (instant diagnosis) and sonnet agent (R6 lattice
  rewrite). kimi railB still running (observed mid rail-4 in its err log).

### Explore diagnosis returned; opus fix dispatched
- Full entry-path map with file:line landed (see R8 diagnosis section).
  Three-leg fix specced and handed to an opus agent in an isolated worktree:
  (1) apply_process_budget on every CLI entry (extracted from
  apply_daemon_budget; DL_NO_BUDGET escape, DL_BUDGET_DEBUG verify line),
  (2) DL_MAX_WALL_SECS watchdog (default 300s) on one-shot in-process runs,
  exits 124 stating phase/detail/root per the self-diagnosis law,
  (3) 10s read-timeout loop on the daemon-attach client printing
  `waiting on daemon (Ns): <phase>`, exit 75 after budget.
  Receipt required: hermetic, budget line + nice=10 + forced watchdog trip;
  the full in-process sprefa rebuild reproduction deferred until user is back.
- Noted for later: ambient-config repos being added to every ad-hoc run is a
  separate design decision, deliberately NOT changed in this fix.

### R6 round 1 returned
- measures-proto.dl rewritten with the reach_from(src, callee, d)
  key(src, callee) merge(MinBy(d)) lattice, depth cap 32. 4.73s full run on
  the 153-file scratch snapshot; reach_from 77,763 rows from 6,564 call_edge
  rows, db +131KB. Findings 3 and 4 above; verdict material awaits user
  (blast vs fan_in overlap is the main keep/kill call). File left uncommitted;
  the live daemon's doc-gen rail picked it up and touched README.md +
  docs/reference/examples.md (expected, commit together later).
- New task #9: empty-scan-scope wipe guard (Finding 4).

### kimi railB: files landed, run cut by connection error, resumed
- All four rails + scripts/rails-oracle.sh exist on disk. kimi's run died on a
  provider connection error mid-oracle-iteration (exit 1); resumed its session
  (session_fd7b1f05) with the remaining steps: dishonest-flag msg update,
  oracle rail-1 HEAD filter, full oracle re-run, final report.
- Fable review of the rail files (pre-oracle):
  - unordered-select.dl: scalar-token exclusion window (+3 lines) and 10-line
    waiver window are approximate but fine for warning tier.
  - dishonest-flag.dl: kimi deliberately tightened the honest signal to
    `rows_changed` only — `moved`/`.get()` NOT counted honest, matching RCA
    defect 2's actual fix shape. Extra HEAD findings under that stricter rule
    are audit material, kept unwaivered.
  - lossy-dedup.dl / static-n1.dl: straightforward sg/regex + waiver comments.
  - All four are warning severity, so pre-commit (`dl --check`, blocks on
    error only) stays non-annoying; they surface as audit lists.
- Commit decision deferred until the oracle PASS table arrives.

### kimi railB finished after resume; oracle PASS; Fable re-running it
- kimi's final report: all five oracle assertions PASS. HEAD finding counts:
  unordered-select 48, dishonest-change-flag 25, lossy-dedup 40,
  static-n1-write-loop 1. cargo test 835 passed. No commits.
- Fable review of rails-oracle.sh: assertions are genuine (grep for each
  historical file in each old-rev worktree run). Found one vacuity hole —
  if the old-rev SQL-fragment capture parses nothing, the rail-1 HEAD-clean
  loop passes trivially. Patched: empty capture now fails the assertion.
- Re-running the patched oracle as the R1 acceptance gate before committing
  the four rails + script.

### R1 closed; R2 dispatched
- Patched oracle re-run by Fable: all five assertions PASS. Committed the four
  rails + rails-oracle.sh as **792cc902**. Task #1 completed. (Rails saga:
  railA a45c34d9 + railB 792cc902 = the full prevention set from the RCA.)
- kimi (free again) dispatched on R2: unconditional cheap orphan probe at the
  top of drain_effects — orphaned kinds whose rel_effect_cmd template now
  exists get one batched UPDATE to queued and drain in the same call; failed
  stays terminal; one it-test. Spec: scratchpad/kimi-r2-spec.md.
- Live receipt for R2 (orphaned -> queued -> done on a real boot) needs a
  daemon stop window; deferred until the user is back, like the R3 force-kill
  receipt.

### Commits 67ed59fe (R2) + e7d29829 (R8); live receipt underway
- Split the shared-tree diff by hunk (the two rel_effect_cmd_txt one-liners
  staged into the R2 commit via an extracted patch) — two scoped commits:
  67ed59fe (effect templates + orphan re-queue) and e7d29829 (process budget,
  watchdog, attach-wait bound).
- Checked the `dl daemon load-once` budget edge: it is an RPC to the daemon,
  budgeted daemon-side; no gap.
- `dl daemon why`: daemon pid 29724 exited CLEAN on its own (idle timeout),
  61m ago. Clean window for the live receipt: nice-limited
  `cargo install --path .` running; then run `dl` from ~/projects/instant
  exactly as the user does (autostarts the new budgeted daemon, budgeted
  one-shot client, bounded attach waits).
- Open observation, not yet a task: the daemon's last sink job
  (sink:fbabddda40d22347) read 10.0GB / wrote 3.3GB over its run before the
  clean exit. Worth a look at what that sink was doing — parked for the
  user's return.

### Live receipts landed: R8 CLOSED, R2 CLOSED
- Instant receipt (new binary, bare `dl` from ~/projects/instant): [budget]
  line, 10s phase-naming heartbeats, result at 71.04s wall; client process
  0.03s user cpu / 10MB rss. Cold worst case (exe swap + new root + genuine
  program-edit rebuild from the 4 committed rails). Settle check 3min later:
  phase=idle, 46s idle window, rss 80MB. Boot total 3.2GB written — explained
  by the genuine program-edit rebuild (known open arc: derived-layer content
  skip), not a recurrence.
- R2 live receipt: after this cold boot, pending_effect in the sprefa root db
  = 6 rows all state=done. Pre-fix behavior was 5 rows re-parked orphaned at
  EVERY boot. Tasks #2 and #8 completed.

### R3 live force-kill receipt run; one regression found (task #10)
- Procedure: appended a comment to .dl/file-size.dl (program-edit trigger),
  watched why.jsonl until the sprefa root hit phase=derived (mid
  port_of_reach_rec_seed), kill -9 38495, restarted.
- Result: recovery was clean — the pending program-edit rebuild re-ran
  (correct: the trigger had not been consumed), root settled, effects stayed
  done, and follow-up ticks were incremental at 907/613/323ms. NO
  self-perpetuating storm, which was the original crash-window disease.
- Caveat recorded honestly: the "kill costs exactly one component" claim is
  NOT separately provable from this trigger, because a kill mid-program-edit
  legitimately re-runs the whole program rebuild; the component scoping is
  pinned by the landed it-tests instead. A scoped-trigger live kill is too
  racy to catch (scoped derived passes are sub-second).
- Regression found while reading the evidence, filed as task #10: why.jsonl
  samples carry root:"" and roots/<hash>/perf.jsonl files do not exist — all
  perf lines route to the daemon home's .dl/perf.jsonl with no root field, so
  full_reason is unattributable per root. The per-root routing from the
  perflog rewrite is not engaging on the serve path.
- Trigger comment reverted; task #3 closed.

### R6 closed (1c3ff4a6), #9 committed (c3c587c9), R4+#10 dispatched, R7 specced
- R6 round 2 (smashy) returned: zero structural edits needed, thresholds and
  TS sparsity are the portability lessons. docs/arch-measures-review.md holds
  both rounds + open verdicts. Committed 1c3ff4a6 (incl. doc-gen README/
  examples hunks). Task #6 completed.
- kimi #9 guard reviewed + committed c3c587c9 (both distinctions pinned:
  no-scan-rules skips, zero-match still reconciles). Task #9 completed.
- Two kimi workers now running in parallel on disjoint files: #10 (per-root
  perf/why attribution, spec kimi-t10-spec.md) and R4 (per-tick _write_ledger
  at the two RCA seams, spec kimi-r4-spec.md, bookkeeping-excluded from
  settle, batched flush, 200-tick retention).
- R7 written up as a spec for user sign-off, NOT implemented:
  plans/2026-07-17-diag-stage-routing.md — diag_stage(code, stage) rel,
  colocated routing, default warning->commit-only, agent-turn intersects
  agent_edit, three open questions at the bottom.

### #10 closed (c33ffc04)
- kimi's root cause: the tick-scoped root was cleared at end_tick BEFORE the
  sink-drain half of a job ran, so drain-phase samples/perf fell back to the
  daemon home. Fix: refcounted root stack + RootGuard held for the whole job
  (daemon.rs:1390); begin/end_tick push/pop the same stack; revert pinned by
  a unit test. Perf lines land in <root>/.dl/perf.jsonl.
- Residual accepted and recorded: the single tick_root pairing slot can
  mispair if two ticks ever BEGIN concurrently on different threads — the
  process-global slot is inherently approximate; the guard makes it strictly
  less wrong. True fix would be job-context plumbing; not worth it today.

### R4 closed (4e87840f); session close-out
- kimi's _write_ledger reviewed: two-seam capture, batched flush, bookkeeping
  registration, settled-tick-writes-nothing pinned by test. One review catch:
  kimi bumped SCHEMA_EPOCH to 11, which drops every rel table on mismatch =
  needless blank-slate rebuild of all served roots; reverted to 10 (the
  CREATE IF NOT EXISTS block runs on every open, so the table lands anyway).
  Targeted tests re-run green, then committed.
- Final full suite verification running in background; CLAUDE.md ledger
  updated (exe-swap arc closed with receipts + rails, effects mystery closed,
  4 new open one-liners).

### kimi R2 returned — with a root-cause bonus; opus worktree isolation broke
- kimi's fix reviewed and accepted pending commit: unconditional orphan probe
  in requeue_orphaned_effects (batched IN-list UPDATE, failed untouched) PLUS
  a real root-cause find: both executor-template call sites read
  `rel_effect_cmd` raw, whose columns are interned INTEGER ids — `as_str()`
  returned None, so DYNAMIC effect templates were invisible to
  exec.has_template. That is very likely the boot-time template race itself.
  Fixed by reading the generated `rel_effect_cmd_txt` view (src/lower.rs:7
  txt_tbl — every rel has one). Convergence checked: exec and the probe now
  read the same view, so no orphaned->queued->orphaned ping-pong that would
  break settle. New it-test asserts orphan -> queued -> done with no other
  effect queued. cargo test 840 passed (includes opus's in-flight tests, see
  below).
- Process note: the opus agent LOST WORKTREE ISOLATION when resumed after its
  connection drop (same failure mode as earlier in the saga) — its R8 edits
  (apply_process_budget, read_frame_watched, watchdog module, cli/mod.rs,
  why.rs) are landing in the MAIN tree interleaved with kimi's R2 edits.
  Files coexist without conflict (different regions). Plan: when opus reports,
  review the combined diff, then commit in two scoped commits (R2:
  effect.rs + temporal_async.rs + the two _txt hunks; R8: the rest).
