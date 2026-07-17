# Auto-session log — 2026-07-17

Persistent progress doc for the driving-window autonomous session. The task
list (R1-R8) is the queue; this file is the narrative record you read later.

**How to read this doc:** start with "The story so far" right below — it is
written to be read cold, no context, each finding explained gently with the
why before the what. The tables and History further down are the terse
bookkeeping version of the same events.

## The story so far (plain language, newest at the bottom)

**Where we were when you started driving.** Over the last three days we chased
down why installing a new `dl` binary made your laptop beachball: six separate
bugs stacked on top of each other, the deepest being that the code extractor
rolled dice — two runs over the exact same files produced different rows, so
the engine honestly believed the data changed and rebuilt everything. That is
fixed and receipted (4.7GB per boot down to 111MB). The write-up lives in
docs/rca-exe-swap-write-storm.md if you ever want the full war story.

**What this session is doing.** Two things in parallel. First, building
"rails": automated tripwires that fail loudly if anyone (including us) ever
writes those same bug shapes again. Second, your newest problem — running
`dl` from your ~/projects/instant repo locks up the terminal — jumped the
queue and is being diagnosed right now.

**Finding 1 — the determinism rail is real, and I checked it can't lie.**
kimi wrote a test that builds a tiny fake codebase, extracts facts from it
twice, and demands byte-identical results. The subtle risk with a test like
this: if the "force the second extraction" step silently doesn't work, the
test compares a run against itself and always passes — a smoke detector with
no battery. I traced the database keys it deletes to force the re-run and
confirmed they are the real ones the engine checks. The rail is live,
committed as a45c34d9.

**Why your instant terminal freezes (hypothesis, being verified).** The
daemon — the long-running background version of dl — got strict resource
manners this week: low CPU priority, throttled disk, capped threads. But all
of that is applied at the daemon's front door only. When you run `dl` directly
in a terminal, the work happens in that process, which never walks through
that door: it gets every core, full-speed disk, and no time limit. On a repo
dl has never indexed, that can mean scanning everything under the directory
at full blast. An agent is currently mapping every way `dl` can start work
and what it actually did in your instant repo; the fix will make one-shot
runs obey the same budget plus a hard timeout.

**Finding 2 — the instant diagnosis came back, and the hypothesis held, with
three sharper details.** The agent mapped every way a typed `dl` command can
run and found: (a) when `dl` does the work in your terminal process instead
of handing it to the daemon, it gets the daemon's thread cap (2 threads —
which is exactly your observed ~50% cpu on 4 cores) but NONE of the disk
throttle or low priority. The code itself contains a comment admitting the
disk throttle is the only thing standing between a cold rebuild and a
beachball; one-shot runs never get it. (b) It doesn't just scan instant:
any ad-hoc run quietly ADDS every repo from your global config on top of the
directory you ran it in, so "index this little 203-file repo" becomes "also
re-chew sprefa". (c) There is no time limit anywhere on that path, and one
more trap: if `dl` tries to hand work to the daemon while the daemon is busy,
the wait for a reply has no timeout either — your command just hangs
silently, indistinguishable from a freeze. Fix has three legs: same budget
caps for one-shot runs, a hard wall-clock deadline that dies loudly saying
what it was doing, and a reply timeout that reports what the daemon is busy
with instead of hanging. Being implemented by an opus agent now.

**Finding 3 — the architecture measures ran, and the data is interesting.**
Recall the goal: express "which code is expensive to change" as queries over
dl's own fact tables, using this repo as the guinea pig. Four measures ran on
a snapshot of the real index in under 5 seconds: *fan-out* (how many functions
does X call — high means orchestrator, split candidate) top-10 is exactly the
daemon's tick/dispatch entry points, which is the right answer. *Fan-in* (how
many call X — high means its signature is expensive to touch) top-10 is the
tiny utility primitives (TypeArena.get, Parser.next, Db.conn), also the right
answer. *Blast radius* (transitive dependents, the fancy recursive one)
mostly re-ranked the same names fan-in already found — a keep/kill question
for you: it may not earn its cost at top-10 depth. *Cycle detection* found
exactly one mutual-recursion cluster, the TypeScript AST visitor in
typegraph.rs — a true recursive algorithm, the "bless it" case rather than an
accident, which means the measure works. Also: the engine's ban on
materializing full closures forced the rewrite into the depth-capped lattice
idiom, and it stayed small — 78k rows, +131KB, no explosion. Full tables:
scratchpad r6-measures-round1.md.

**Finding 4 — the measures run tripped over a new engine footgun.** Running
dl standalone against a copy of the database, with a query file that declares
no file scan of its own, silently ERASES the copy's file-derived tables: the
engine treats "this program scans nothing" as "the set of files is truly
empty" and dutifully reconciles everything down to zero. On a scratch copy
that's confusing; on a real db it would be data loss. The agent proved it on
two fresh snapshots, worked around it by adding a one-line scan to the query
file, and I've queued a proper guard (engine should refuse or warn when a
program's scan scope is empty but the db already has files). New task #9.

**Finding 5 — all the storm rails are now in and proven against history.**
The four "never write this shape again" tripwires are committed, and the part
you insisted on is done: an oracle script checks out the actual pre-fix
commits and confirms each rail catches the actual historical defect it was
built from — the unordered-query rail flags all five files from before the
determinism fix, the dishonest-flag rail flags the three files that used to
hardcode "yes, things changed", and the lossy-dedup rail keeps today's known
lossy sites on an audit list. I found and closed one hole in the oracle
itself (a case where an empty capture would have made one check pass without
testing anything), re-ran it clean, and committed everything as 792cc902.
The rails are warning-severity, so they will not block your commits — they
produce audit lists (48/25/40/1 findings on today's code) for deliberate
triage later.

**Finding 6 — the "orphaned effects" mystery has a real root cause, found by
accident.** Background: some background commands ("effects") kept getting
stranded at every daemon boot, and we assumed a startup timing race. While
implementing the planned workaround (re-check stranded effects each cycle),
kimi hit the actual cause: the effect templates defined in your dl programs
are stored in the database as numeric string-ids (everything in the engine is
interned), but the code that loads them asked for them as text and silently
got nothing back. So dynamically-defined effect commands were invisible —
every boot, the engine concluded "no template exists for this" and parked the
effect. The fix reads the text-resolved view instead, at both loading sites,
plus the planned re-check so old strandees recover on their own. A new test
pins the full recovery path. Suite green (840 tests).

**Finding 7 — your instant problem is fixed, receipt in hand.** I installed
the fixed binary and ran plain `dl` from ~/projects/instant, exactly your
invocation. What used to be a frozen terminal is now: one line confirming the
resource caps are on, then a heartbeat every 10 seconds telling you what the
engine is chewing on ("waiting on daemon (40s): derived call_target"...),
then your query result at 71 seconds — and that 71s was the worst case, a
cold boot where the daemon re-indexed everything on a new binary. Your
terminal process itself used 0.03s of CPU and 10MB of memory; all heavy work
happened in the background daemon at low priority with throttled disk. Next
runs attach warm and answer immediately. If anything ever does wedge, the run
kills itself at 5 minutes and prints what it was doing (tune with
DL_MAX_WALL_SECS). Committed as e7d29829.

**Finding 8 — I crash-tested the daemon on purpose, and it shrugged.** With
everything settled I triggered a big rebuild, waited until the daemon was
mid-way through the heaviest phase, and killed it with the OS's most violent
signal (the one that allows no cleanup — same as your force-quits). Restarted
it. The old disease was that every kill armed the next boot to redo
everything forever; this time it finished the interrupted work once, settled,
and follow-up cycles dropped to under a second. The stranded-effects check
from Finding 6 also held through the crash. One honest caveat: I couldn't
cleanly measure the "a crash costs only the one interrupted piece" guarantee
live, because the trigger I used legitimately requires a full redo — that
guarantee stays pinned by the test suite. Bonus: reading the crash evidence
exposed that per-project performance logs stopped being filed per-project
(everything lands in one shared file with no project label). Filed as task
#10, not urgent, but it would have made tonight's forensics one query
instead of three.

**Finding 9 — the measures work is wrapped, and the portability answer is
"yes, with two lessons".** The same measures file ran on smashy (the
TypeScript game repo) without changing a single rule — that's the "take this
analysis anywhere" property you wanted, confirmed. The two lessons: fixed
number thresholds don't travel (smashy's whole call graph is smaller than
sprefa's top function, so every "hot" list came back empty until rescaled —
a portable version needs top-K or percentiles instead), and TypeScript call
graphs come out 4x sparser than Rust's, which ties to the known gap where
TS class-method bodies extract nothing. Everything — both rounds' data, per
measure keep/kill questions, and the concrete next steps if you keep them —
is in docs/arch-measures-review.md, committed as 1c3ff4a6 along with the
prototype. The database-wipe footgun from Finding 4 is also fixed properly
in the engine now (c3c587c9): a query-only program warns and leaves your
data alone instead of erasing it.

**Finding 10 — the engine now keeps receipts on itself.** The single biggest
"we had to dig for it" cost of the storm saga was that nothing recorded which
relation wrote how many rows each cycle — attribution came from correlating
an OS sampler against timestamps. That ledger now exists: every cycle, every
writer that actually wrote rows is recorded (batched, self-pruning, and
excluded from the "are we settled" check so it can't cause the perpetual-tick
bug it is meant to help debug). Next time anything writes gigabytes, one SQL
query names the culprit. I also caught and reverted one thing in kimi's
version before committing: it bumped the database schema version, which
would have silently forced a from-scratch rebuild of every project you serve
at next boot — unnecessary, since the table creates itself on existing dbs.
Committed as 4e87840f.

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
