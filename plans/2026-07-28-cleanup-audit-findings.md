# OVERNIGHT CLEANUP AUDIT — FINDINGS (2026-07-28)

Contract: `plans/2026-07-28-overnight-cleanup-audit.md`.
Range audited: `99571613..363b4d3a` (83 commits, 260 files, +17,936 / -2,210),
i.e. everything on `cleanup/2026-07-27-reconcile` since 2026-07-27 20:00.

Entry state re-verified in this worktree before any edit: v6/dl typecheck clean
+ 90/90, one-subscribe 1, v6/sprefa-store/js 89/89, v6/tsv2 6/6, tsv2 import
gate OK.

Ownership honored: edits confined to `v6/dl/**`, `v6/tsv2/**`, `v6/tools/**`.
`v6/prolog/compile/**` and `v6/prolog/conformance/**` read only. No file named
`parse_dl.pl` / `print_dl.pl` / `SYNTAX.md` and no `dl_view/` created.

Counts: **24 test cases** audited (new or changed in range), **11 sabotage
probes run**, **3 confirmed false positives**, **1 confirmed vacuous
assertion**, **2 redundancies** (both kept, justified below), **0 tests
removed**, **7 mechanical fixes applied** in one commit.

---

## Applied fixes

| commit | file | change |
|---|---|---|
| `506cb0b8` | `v6/tools/one-subscribe.sh` | exclusion tightened from the `*Channel` wildcard to the four literal `0_trace.ts` handle names; count floored at 1; stale "lower BASELINE to 1" line dropped |
| `506cb0b8` | `v6/tsv2/scripts/check-imports.sh` | reuse-law check reads **import lines** instead of whole-file text; direct `@libsql` import refused; `gen_emitted/*.ts` now gated alongside `gen/*.ts` |
| `506cb0b8` | `v6/dl/tests/2_helpers_binds.ts` | dead `bridgeBindsFixture` (zero call sites) + its now-unused `bridge`/`readFixture`/`builtinRelsForTests` imports deleted |
| `506cb0b8` | `v6/dl/src/3_runtime.ts` | `rowsForPath$` docblock corrected: it claimed the method is NOT on `IDlRuntime` and that `4_ingest.ts` probes with `instanceof DlRuntime`; both false as shipped |
| `506cb0b8` | `v6/dl/src/0_types.ts` | "Three seams" -> four; "any of the three channels" -> four (stale since the bind seam landed) |
| `506cb0b8` | `v6/dl/src/0_trace.ts` | "across all three seams" -> four |
| `506cb0b8` | `v6/tsv2/scripts/run-fixture.ts` | comment claimed to be "the one manual `.subscribe()` in this app" while `run-emitted.ts` and `sweep.ts` each hold their own; corrected to name all three |

Post-fix receipts: v6/dl typecheck clean + 90/90, one-subscribe 1; v6/tsv2
typecheck clean + 6/6, import gate OK. No behavior changed.

---

## Task B — per-test table

`can it fail?` column: **probed** = the guarded code was reverted or sabotaged
in this worktree and the test's colour recorded, then the file restored with
`git checkout`. **inspected** = judged by reading, no sabotage run.

### v6/dl/tests/0_trace.test.ts (NEW, 3 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| off by default: channels carry no subscriber, no file written | 3x `hasSubscribers === false`, `PerfTrace.enabled === false`, `fs.existsSync(logPath) === false` across a real commit | **probed**: made `installFromEnv` fall back to a default path -> RED (at the `hasSubscribers` line) | KEEP, with one **vacuous assertion** — see F5 |
| on: real commit emits one parseable JSONL line with the contract fields | one line per tick, every `PerfTickLine` field present and typed, `stmt_count > 0`, `wall_ms >= stmt_ms_max` | **probed**: `logger.info(line)` removed -> RED | KEEP |
| on: effectDone/ingestDone fold into the same tick's line | exact `effects[0]` and `ingest` object shapes for a manufactured tick 42 | **probed**: same sabotage -> RED | KEEP; does **not** cover `binds` (F4) |

### v6/dl/tests/3_runtime.test.ts (+5 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| `rowsForPath == rows().filter(path)` for every distinct path | set equality between the scoped read and the unscoped read + JS filter, for 3 paths + one absent path | **probed twice**: (a) `WHERE <col> = <id>` -> `WHERE 1=1` -> RED; (b) whole implementation reverted to `selectAll(...).filter(...)` -> **GREEN** | KEEP for correctness, but see **F1**: it does not discriminate the arc's headline perf property |
| `rowsForPath` throws on an unknown rel | `/unknown rel/` rejection | inspected (message-matched, single code path) | KEEP |
| `rowsForPath` throws when the rel has no `path` column | `/no column 'path'/` rejection | inspected | KEEP |
| commit's guard rejects a non-finite number, naming rel + column | error off `deltas$` matches `/status_rel/` and `/column 'status'/` | **probed**: `Number.isFinite` guard deleted from `encodeSurfaceRowByColumns` -> RED (the surviving `execute$` message names the rel but not `column 'status'`) | KEEP; the test's own comment documents **F2** |
| `execute$` carries the failing statement's text (truncated) | `/execute\$ failed on statement:/`, `/SELECT x{10,}/`, message shorter than the 2000-char SQL, `cause` present | inspected (four independent assertions, all on the new `catchError`) | KEEP |

### v6/dl/tests/4_binds.test.ts (NEW, 3 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| clockBind fires on its declared period and `rel(1)` keeps only the latest bucket | two strictly increasing buckets, table stays at 1 row, `poll_due` mirrors the latest | inspected + implied by the activation probe | KEEP; wall-clock (F6) |
| clockBind stays inactive when the program never declares `clock_bucket` | `commits$` emits 0 over a 1.5s window | **probed**: activation filter removed (`activeBinds = binds`) -> RED | KEEP; wall-clock (F6) |
| unsubscribing `commits$` stops the timer — no bespoke `dispose()` | `clock_bucket` rows unchanged over a 2.5s window after `running.unsubscribe()` | **probed**: `BindRunner` given a held internal subscription keeping every timer alive past the external unsubscribe -> **GREEN** | **CONFIRMED FALSE POSITIVE — F3** |

### v6/dl/tests/4_hosts.test.ts (+2 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| multi-line sh output (>1 output col) parses to ONE row | exactly 1 row, `status: 200` as a number, tag and multi-word body intact | **probed**: the `lines.length === responseCols.length` branch disabled -> RED | KEEP (the tight unit guard) |
| F7 regression end to end: fetch/resp/stars/full_name/change_log | 1 `resp` row, 4KB+ JSON body that `JSON.parse`s, `stars`/`full_name`/`change_log` all land | **probed**: same sabotage -> RED | KEEP despite overlap — see **R1** |

### v6/dl/tests/6_binds_http.test.ts (NEW, 1 case / 3 legs)

| leg | asserts | can it fail? | verdict |
|---|---|---|---|
| clock_bucket advances on its own | two strictly increasing buckets over http, nothing POSTed to `/edb/clock_bucket` | inspected; the activation probe covers the same path | KEEP |
| poll_due re-fires on SSE | `>= 2` `data:` lines on `GET /subscribe/poll_due` | inspected | KEEP |
| reload stops the timer | `clock_bucket` snapshot unchanged across a 3s window after loading a program with no `clock_period` | **probed**: held-internal-subscription sabotage -> **GREEN** | **CONFIRMED FALSE POSITIVE — F3** |

### v6/dl/tests/6_http.test.ts (+2 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| HOL regression: a stalled program POST does not block a later one | second POST answers 200 within 5s while a Content-Length-lying request hangs | **probed**: `accepted$`'s `mergeMap` -> `concatMap` -> RED (5s timeout) | KEEP |
| SSE regression: a program reload ends the client's response stream | `res` `"end"` fires within 5s of the reload, `activeSubscribeCount` back to 0 | **probed**: `response.end()` removed from `sseClient$`'s `finalize` -> RED | KEEP |

### v6/tsv2/tests/diff.test.ts (NEW, 3 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| plain set-style add and del | `add=[c]`, `del=[a]` | inspected | KEEP |
| unchanged rows produce no delta | `{add:[],del:[]}` | inspected (weak, but it is the identity case) | KEEP |
| duplicate row values are counted, not deduped (Log append) | 1 -> 3 occurrences yields 2 adds | **probed**: multiplicity loop collapsed to a single push -> RED | KEEP; **it is the only guard for multiset semantics** (see F8) |

### v6/tsv2/tests/tickLoop.test.ts (NEW, 3 cases)

| test | asserts | can it fail? | verdict |
|---|---|---|---|
| `demand_laziness_effect_rows` matches the oracle tick log | byte-equal JSONL vs the prolog-generated fixture, 5 lines | inspected; oracle is external ground truth | KEEP |
| `switch_as_keyed_replace` matches the oracle, including the drain tick | byte-equal, 3 lines, tick 3 is the empty drain line | **probed**: `carryPending` drain disabled -> RED | KEEP |
| PERTURBED schedule matches the oracle's perturbed log | byte-equal against a schedule no fixture Expectations covers, plus `/"gamma"/` | inspected; this is the anti-replay control and it earns its place | KEEP |

### Helpers changed in range (no assertions of their own)

| file | change | verdict |
|---|---|---|
| `tests/1_helpers_db.ts` | `bootFixture` now subscribes `rt.deltas$` (the tick loop only turns while something subscribes) | correct; subscription is intentionally unheld, `rt.dispose()` completes it |
| `tests/2_helpers_hosts.ts` | `runner.start()/dispose()` -> one `merge(deltas$, effects$)` subscription | correct, mirrors `main.ts` |
| `tests/2_helpers_binds.ts` | NEW; `bootBindRunnerFixture`/`disposeBindFixture` | dead `bridgeBindsFixture` deleted this pass |
| `v6/sprefa-store/js/tests/engine/txn.test.ts` | `SqlRunner.run` -> `SqlRunner.execute` (the deleted method) | mechanical rename, no semantic change |
| `v6/tsv2/tests/schedules.ts` | NEW; three arrival schedules as data | fixture data, no assertions |

---

## Redundancy verdicts (both kept)

**R1 — the two F7 tests overlap on the parse fix.** Both go red under the same
one-line sabotage. They are NOT redundant overall: the unit test pins the
parser's exact output shape with no subprocess dependency beyond `printf`; the
end-to-end test additionally exercises the SQL splice site, the `jq` reshape
hosts, and the `resp -> stars/full_name -> change_log` derived chain (the
"post-response tick" receipt). **Keep both. If one must go, keep the unit
test** — the e2e one also depends on `jq` and `seq` being on PATH, which is an
environment coupling no other test in the suite has.

**R2 — the bind teardown property is asserted twice** (`4_binds.test.ts` case 3
and `6_binds_http.test.ts` leg 3). Both are confirmed false positives (F3), so
this is a redundancy of two non-discriminating tests. **Neither should be
deleted; both should be repaired** per F3, at which point the unit-level one
becomes the primary and the http one keeps only its distinct value (that
`switchMap`, specifically, is the thing that tears the branch down).

---

## Task C — baseline / ratchet table

| baseline | what it guards | false-positive risk (passes while broken) | false-negative risk | tightening |
|---|---|---|---|---|
| `v6/tools/one-subscribe.sh` (baseline 1) | exactly one manual rxjs `.subscribe()` in `dl/src` | **WAS REAL, REPRODUCED**: `grep -v 'Channel\.subscribe('` excluded any variable whose name ended in `Channel`; `notifyChannel.subscribe()` in `dl/src` exited 0. **FIXED** (`506cb0b8`), re-probe now exits 2. Remaining: `firstValueFrom`/`lastValueFrom` subscribe internally and are invisible to the grep (accepted idiom, 3 sites) | **WAS REAL, REPRODUCED**: pointing the scan at a nonexistent dir gave 0 sites and exit 0 — a rename would have silently voided the ratchet. **FIXED**: count floored at 1 | **DONE** for both. **NOT done**: the scan covers `dl/src` only; `v6/tsv2` and `v6/sprefa-store/js/src` hold **11** rxjs `.subscribe()` sites the law never sees (F7) |
| `v6/tsv2/scripts/check-imports.sh` | gen files import only `../runtime/` + rxjs; runtime/ reuses the named store symbols (class-34) | **WAS REAL, REPRODUCED**: whole-file `grep -rq "$symbol" runtime/*.ts` matched the symbol in `scratchStore.ts`'s own doc comment, so deleting both real imports and constructing `@libsql/client` directly left the gate GREEN. **FIXED** (`506cb0b8`); re-probe now exits 1 with two named failures | `grep -oE 'from "[^"]+"'` still misses side-effect `import "..."`, dynamic `import(...)`, and single-quoted specifiers. `gen_emitted/` was entirely unscanned — **FIXED** | Remaining tightening: parse specifiers with a real ES-module scan rather than a `from "..."` regex (F9) |
| `.dl/no-new-eprintln.dl` (v5) | no new `eprintln!` in `src/**/*.rs` | scans Rust only; the whole v6 TypeScript tree is out of scope by design | a file with no baseline row warns per hit, so a NEW file's prints are caught | none needed for its stated scope; note only that "diagnostics go through tracing" has **no v6 equivalent** — `console.error` in `engine.ts:148` is unguarded (F10) |
| `v6/dl/tests/conformance.test.ts` golden | every language case, asserted on the resulting SQLite | standard golden-file hazard: `REGEN_GOLDEN=1` rewrites the expectation, so a behavior change committed together with a regenerated golden rubber-stamps itself | genuinely broad (31 rels) | require the golden's diff to appear in its own commit, or hash-pin the golden in the test |
| `v6/prolog/compile/SCOREBOARD.md` buckets (109 = 92/9/8) | which fixtures compile identically | REPORT-ONLY (not edited): the scoreboard is regenerated by `sweep.sh`, so it records rather than gates — nothing fails a build when a bucket regresses. 4 fixtures are noted as passing on an empty schedule (vacuous) | retention `keep(count)` is not lowered AND invisible to tick-log-only grading, already recorded in the ledger | make `sweep.ts` exit nonzero when the IDENTICAL count drops below a committed floor |
| `v6/tsv2` oracle fixtures (3 JSONL) | tsv2 runtime vs the prolog oracle | narrow corpus: **the two committed programs do not exercise duplicate-row Log append at all** — a multiset-diff sabotage that breaks it leaves all three oracle tests green (probed) | the perturbed-schedule case is a real anti-replay control | add one oracle fixture whose log contains a repeated row value (F8) |
| ARCH.pl `go` (7/7) | architecture map coherence | REPORT-ONLY | — | — |

---

## Ranked semantic findings (no fix applied; each needs a call)

### F1 — the perf arc's headline property has zero test discrimination (HIGH)
`v6/dl/src/3_runtime.ts:375-403` (`selectByColumn`), `v6/dl/src/4_ingest.ts:324`.
The arc's receipt is `diff_ms 3676 -> 16ms flat, 13.3 -> 74.2 files/s`, bought
by pushing the `path` predicate into SQL against the interned column.
**Probed**: I replaced `rowsForPath$`'s body with
`selectAll(...).pipe(map(rows => rows.filter(row => row.path === path)))` — the
exact O(n²) shape the arc removed — and **all 19 `3_runtime` tests and all 9
`4_ingest` tests stayed green.** The only test on this path asserts set
equality between the two reads, which is precisely the property the slow
implementation also has.
*Failure scenario*: any later refactor of `selectByColumn` (a decode-view
change, a column-type widening, a "simplify this" pass) silently restores
superlinear per-file ingest, the suite stays green, and the regression is found
only by re-running `ingest_corpus` by hand.
*Suggested shape*: assert a **statement/row budget**, not equality — e.g. commit
N paths, then assert `rowsForPath` returns in a row count independent of N, or
count statements through the existing `stmt_counter` (the store already has
`tests/lower/stmtBudget.test.ts` as the precedent).

### F2 — a tick-pipeline exception makes `commit()` hang forever (HIGH)
`v6/dl/src/3_runtime.ts:969-1000` (`commit`; the refusal is line 972),
documented in the test at
`v6/dl/tests/3_runtime.test.ts:494-508`.
`commit()` settles through `reportsSubject`; a fault raised inside the
`commits$ -> applyEdbTxn -> applyDerivedTxn` chain travels on `deltas$` and
never reaches `reportsSubject`. So the caller's promise neither resolves nor
rejects. **Probed**: with the `commits$.observed` refusal removed, running
`4_binds.test.ts` hung past a 600s timeout instead of failing.
The NaN-guard test works around this by observing the error off `deltas$` and
`.catch(() => {})`-ing the commit — the workaround is honestly commented, which
is how the defect surfaced.
*Failure scenario*: any host effect, bind commit, or http handler that hits a
tick-pipeline fault wedges its caller silently. Under `serveDl` the fault also
kills the whole app stream, so the http client sees a dropped connection with no
diagnostic — a direct hit on the self-diagnosis law.
*Direction*: route pipeline faults back to the in-flight commit id (reject the
promise) as well as onto `deltas$`.

### F3 — both bind-teardown tests are false positives; `commit()`'s refusal masks a leaked timer (HIGH)
`v6/dl/tests/4_binds.test.ts:97-119`, `v6/dl/tests/6_binds_http.test.ts:133-146`,
mechanism in `v6/dl/src/1_binds.ts:146-171` + `v6/dl/src/3_runtime.ts:972`.
**Probed**: giving `BindRunner` a held internal subscription — the exact
pre-standing-plan-item-3 shape, keeping every `interval` alive past the external
unsubscribe — left **both** tests GREEN.
*Why*: both tests assert "no new `clock_bucket` rows after teardown". Teardown
also stops the tick loop, so the leaked timer's `commitOnce` calls
`runtime.commit()`, which throws `nothing is subscribed to deltas$` (or, without
that guard, hangs — F2). Either way no row lands. The tests are green because
of the runtime's commit refusal, not because the timer died.
*Failure scenario*: a `BindRunner` regression that holds a `Subscription` (or a
`shareReplay({refCount:false})` on the clock source) leaks a `setInterval` per
program load. Under repeated program reloads the process accumulates timers
until it is doing nothing but firing dead clocks, and no test says a word.
*Direction*: assert the **timer**, not the row — e.g. inject a `SchedulerLike`
into `clockIntervalFor` and assert its action count returns to zero, or count
`source$` emissions through a probe bind, so the assertion does not route
through `commit()` at all. This is the same injection the F6 proposal wants.

### F4 — the bind trace seam is entirely unasserted (MEDIUM)
`v6/dl/src/0_trace.ts:236-243,280-284`, `v6/dl/src/0_types.ts:PerfBindEntry`,
`v6/dl/src/1_binds.ts:187`.
`PerfTickLine.binds` is produced by `bindDone` and folded by `onBindMessage`,
but **no test anywhere asserts `line.binds`**, and `0_trace.test.ts:68-70`
checks `hasSubscribers` on `sql`/`effect`/`ingest` while omitting `bind`. The
`effect`/`ingest` twin fields are pinned exactly; the bind field is not.
*Failure scenario*: `bindDone`'s arguments drift (rel/rows swapped, ms in the
wrong unit) and every suite stays green; the perf JSONL quietly carries wrong
numbers into the next perf hunt.
*Cheap fix*: extend `0_trace.test.ts`'s third case with a `PerfTrace.bindDone`
call and a `deepEqual` on `line.binds`, plus the fourth `hasSubscribers` line.

### F5 — one structurally unfalsifiable assertion (MEDIUM)
`v6/dl/tests/0_trace.test.ts:75,80`.
```ts
const logPath = freshLogPath();          // a fresh random /tmp path
...
assert.equal(fs.existsSync(logPath), false);
```
`logPath` is minted inside the test and handed to nothing. The assertion can
only fail if the code independently guessed a 16-hex-digit random filename. It
cannot detect tracing writing to any *other* destination.
The surrounding test is still non-vacuous (probed: the `hasSubscribers` line
goes red when `installFromEnv` gains a default path), so this is an
assertion-level finding, not a test-level one.
*Fix*: point `DL_PERF_LOG` at `logPath`, `uninstall()`, then commit and assert
the file is absent — that version actually discriminates.

### F6 — 12.2s of real wall-clock sleeping in the bind tests (MEDIUM)
`v6/dl/tests/6_binds_http.test.ts` (7.18s measured) and
`v6/dl/tests/4_binds.test.ts` (2.06 + 1.51 + 3.52s measured), driven by
`interval(periodSecs * 1000)` in `v6/dl/src/1_binds.ts:101` with no injection
point. That is **~15% of the whole 8.2s-to-20s dl suite** spent asleep, and
three of the four sleeps are fixed `setTimeout` windows (1500ms, 2500ms, 3000ms)
whose only job is "long enough that a wrong implementation would have fired".
Filed against the pending `SchedulerLike`-injection proposal, per the contract —
**not rewritten here**. Note it is the same injection F3 needs to become
discriminating, so the two should land together.

### F7 — the one-subscribe law is enforced on one of three packages (MEDIUM)
`v6/tools/one-subscribe.sh:14` scans `dl/src` only. Live rxjs `.subscribe()`
sites outside that scan:

| file | line |
|---|---|
| `v6/tsv2/scripts/run-fixture.ts` | 52 |
| `v6/tsv2/scripts/run-emitted.ts` | 76 |
| `v6/tsv2/scripts/sweep.ts` | 143 |
| `v6/sprefa-store/js/src/labs/fixpoint.ts` | 277 |
| `v6/sprefa-store/js/src/labs/prolog.ts` | 265, 494 |
| `v6/sprefa-store/js/src/labs/stress.ts` | 300, 329, 874 |

The three tsv2 ones are defensible (three separate one-shot CLI entry points,
one terminal subscription each — the comment drift is fixed this pass). The six
in `sprefa-store/js/src/labs/**` are lab code and by the **labs-die-on-landing
protocol** should not exist at all in a landed tree. Deciding whether `labs/`
is exempt or is a deletion target is a user call, so no edit was made.

### F8 — the tsv2 oracle corpus does not cover multiset (Log-append) semantics (MEDIUM)
`v6/tsv2/runtime/diff.ts:48-57`.
**Probed**: collapsing the multiplicity loops to a single `push` (turning the
multiset diff into a set diff) leaves **all three `tickLoop.test.ts` oracle
tests GREEN**; only the one unit case in `diff.test.ts` catches it.
*Failure scenario*: the emitter starts producing Log rels whose repeated row
values matter, and the byte-diff grade — the arc's whole correctness story —
cannot see the error, because neither committed fixture has a duplicate row.
*Fix*: one more oracle fixture whose log contains a repeated row value.

### F9 — `check-imports.sh` still parses imports with a regex (LOW)
`v6/tsv2/scripts/check-imports.sh:15`. `grep -oE 'from "[^"]+"'` misses
side-effect `import "..."`, dynamic `import(...)`, and single-quoted specifiers.
This matters because `run-emitted.ts` deliberately loads drafts via a **computed
dynamic import** — so the one escape hatch the codebase already uses is the one
the gate cannot see. The two escapes I could reproduce are fixed; this one is a
real remaining hole and needs a proper module scan, not another regex.

### F10 — v6 has no equivalent of the eprintln law (LOW)
`v6/sprefa-store/js/src/engine/engine.ts:148` prints cascade timings through
`console.error` with no gate, in parallel to the `0_trace.ts` tracing spine that
the perf header names as the single instrumentation seam. The Rust side has a
ratcheted rail for exactly this; the TypeScript side has none.
(`v6/dl/src/main.ts:25-26,32` is the legitimate CLI-UX exception.)

### F11 — `DEFAULT_EXTRACT_BIN` is duplicated, hardcoded, and points at a debug build in a foreign worktree (LOW, already in the ledger)
Byte-identical constant in **two** files:
`v6/dl/src/1_hosts.ts:349-350` and `v6/dl/src/4_ingest.ts:92-93`, both
`/Users/chrishafley/projects/sprefa/.claude/worktrees/extract-golden-plan/v6/sprefa-extract/target/debug/extract`
(the file exists today, 56MB, dated Jul 25 — a **debug** build).
Not fixed mechanically because deduplicating it needs a new shared module: the
numbering law forbids `1_hosts.ts` importing from `4_ingest.ts`, `tasks.d.ts` is
a declaration file and cannot hold a value, and `0_types.ts` is a types header.
The ledger already tracks "release build + in-tree path"; the **duplication** is
the new part — the two copies can drift independently and only one of them is
named in the ledger.

### F12 — the clock bind reads a magic column name (LOW)
`v6/dl/src/1_binds.ts:90` reads `row.period_secs` by literal string. A program
declaring `clock_period(secs: int)` activates the bind and then silently
produces **zero** intervals — `distinctPeriods` finds no finite value and
`source$` returns `EMPTY`, indistinguishable from "no periods configured". This
is the same shape as the `scope_done` read-by-name item already on the
magic-rel-ban list in `CLAUDE.md`. At minimum the mismatch should be loud.

### F13 — `parseWhitespaceColumns`'s row/column heuristic is ambiguous (LOW)
`v6/dl/src/1_hosts.ts:170-176`. The F7 fix reads "line count == output column
count" as one-row-line-per-column. A genuine **multi-row** output that happens to
have as many lines as columns (2 output columns, 2 rows of `a b` / `c d`) is
misparsed into a single row `{col0: "a b", col1: "c d"}`. The fix is correct for
the shape that caused F7 and the comment is honest about the trade, but the
ambiguity is structural and untested in the collision case. A host declaring its
own output arity (JSON) is the escape hatch that already exists.

### F14 — `BindRunner.commits$` is cold and unshared; two subscribers means two timer sets (LOW)
`v6/dl/src/1_binds.ts:163-170` builds `merge(...)` with no `share()`.
`4_binds.test.ts:81` already subscribes it a second time (while
`fixture.running` holds the first), which for the inactive case is harmless
because the source is `EMPTY` — but on an active bind it would spin a second
independent set of intervals and double every commit. `HostRunner.effects$` has
the same shape. Documented as "cold" in both headers, so this is a
sharp-edge note rather than a defect; worth a `share()` or an explicit
one-subscriber-only line in the contract.

---

## Stated goals with NO test discriminating them

| goal (source) | status |
|---|---|
| per-file ingest diff is index-scoped, not whole-table (`v5-port-perf-header.md`; ledger `diff_ms 3676 -> 16`) | **none** — F1, proven by reverting the implementation with the suite green |
| instrumented ingest within 5% wall of uninstrumented (`v5-port-perf-header.md`, "Overhead budget") | **none** — measured once by hand in the arc, never re-measured by any check |
| endurance phase 1: kill -9 mid-delay, value lands exactly once (`goal-endurance.sh` IS the goal definition, CLAUDE.md) | script exists and passes, but it is **not** part of `pnpm test` and no gate runs it; regressions surface only when someone remembers |
| N+1 law on the v6/dl side (CLAUDE.md style notes) | **none** in `v6/dl` — the store has `tests/lower/stmtBudget.test.ts`, `v6/dl` has no statement-budget assertion at all |
| bind perf seam emits correct `binds` entries (`0_types.ts` `PerfBindEntry`) | **none** — F4 |
| bind teardown on unsubscribe (`1_binds.ts` LIFECYCLE header, standing plan item 3) | **two tests, both non-discriminating** — F3 |
| multiset/Log-append semantics under the oracle byte-diff grade (tsv2 phase C) | **unit test only**; the oracle corpus does not reach it — F8 |
| "Diagnostics go through tracing only" applied to v6 TypeScript | **no rail** — F10 |

## Tests asserting something no goal asks for

None found. Every case in the range traces to a named goal: the F7 pair to
failure-modes class 36, the trace tests to the perf header's Phase 0 contract,
the bind tests to the `clock_residency` ruling, the http pair to
`plans/2026-07-27-diff-review-findings.md` items 2+3, the tsv2 tests to the
compile-target header's grading loop, and the `rowsForPath` trio to the ingest
perf arc. The problem in this wave is not misaimed tests; it is tests that name
the right property and then assert a weaker one.
