# v6 world health, 2026-07-29

Written by the world_health_reconcile audit (opus worktree, base `b535ca62`,
folded main through `29977137`). Companion deliverable: the ARCH.pl
reconciliation in the same branch.

Every number below is either (a) re-run by this agent in this worktree, marked
**own run**, or (b) quoted from a committed receipt with its commit named. No
number is quoted from a report. Where a claim has no receipt, it says so.

---

## 1. Battery state

**Own runs**, this worktree, at the folded base:

| gate | result | how |
|---|---|---|
| conformance | **163 PASS / 0 findings** | `swipl -q -l v6/prolog/conformance/go.pl -g go` |
| plunit | **140 / 140** | `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests` |
| TEXT_DOOR | **compiled=102 byte_identical=102 failures=0** | `bash v6/prolog/compile/scripts/text_door_receipt.sh` |
| ARCH `go` | **7 / 7 PASS**, roadmap total | bare swipl and `just arch`, both |

**Sweep artifacts** (`v6/prolog/compile/out/{manifest,run-results}.json`, read
directly rather than re-run):

| bucket | count |
|---|---:|
| fixtures swept | 163 |
| compiled | 102 |
| named refusals (UNSUPPORTED) | 61 |
| of the 102: IDENTICAL to the oracle | 100 |
| of the 102: WRONG | **0** |
| of the 102: run_error | 2 |

**Not runnable here**: this audit worktree has no `node_modules`, so every TS
suite and every shell receipt is out of reach. Last committed receipt for
those is the host-seam landing (`42d11f47`): tsv2 69 pass / 1 skip,
extraction-live HOLDS, leak-soak PASS. The last full-battery statement is the
close-out save (`chat_log/20260729.2`): endurance, extraction-live, lsp-diags,
leak-soak, staleness gate all HOLD, roundtrip ALL PASS.

**Stale receipts, verified stale, not edited** (`v6/justfile` and
`SCOREBOARD.md` belong to whichever lane next touches the sweep):

| file | says | is |
|---|---|---|
| justfile `conformance` | expect 156 | 163 |
| justfile `text-door` | expect 95/95/0 | 102/102/0 |
| justfile `sweep` | expect 95/93/0 | 102/100/0 |
| justfile `plunit` | expect 137/137 | 140/140 |
| justfile `tsv2-test` | expect 65/1 skip | 69/1 skip at `42d11f47` |
| justfile header | "the 135-fixture baseline" | 163 |
| SCOREBOARD totals | 155 / 94 / 92 / 0 / 2 | 163 / 102 / 100 / 0 / 2 |

Nothing is broken by this. It matters because these comments are the only
place a reader learns what a good number looks like, and a reader who trusts
them will read a real regression to 156 as green.

---

## 2. What is genuinely strong

**The oracle is the spec, and it is executable.** 163 fixtures run against
`conformance/engine.pl`, and the compiler is graded by diffing tick logs
byte-for-byte against that engine's own output. 100 of 102 compiled programs
are byte-identical in BOTH emitter modes. This is the rarest thing in the
project: the compiler cannot drift quietly, because drift is a byte diff.

**Two doors, one answer.** Every compiled fixture also compiles from `.dl6`
TEXT through the DCG parser and emits a byte-identical module (102/102/0).
The term form and the surface syntax cannot disagree without the receipt
going red.

**Refusal discipline.** 61 of 163 fixtures are named refusals. Read one way
that is a large unsupported surface; read the other way, the compiler says
which construct it will not lower instead of emitting something plausible.
The three miscompile classes caught this month (TEXT collapse, `@libsql` REAL
bind corruption, cross-type join under affinity) all became refusals or
fixtures rather than folklore.

**Sabotage receipts are the house style now.** The rules that matter carry a
red-then-green witness in the test header: the comment rig flips 745/745 rows
when the column convention shifts by one; the memory soak goes red on
`keep_all`; the LSP column rename passes every engine-side phase and goes red
only at the real v5 client. A rig that cannot go red is not treated as a
grade.

**Where the engine is actually good, it is measurably good.** Retraction as
emitted refCount SQL with a cycle guard beats the rust `sqlite-count` entry on
the shared 1M competition (DAG 960k: 429ms vs 443ms, same input hashes) and is
correct on cycles where the rust bare count is wrong.

**Parity receipts against v5 exist and are byte-exact where the inputs
align.** `comment_node` 745/745 and `arch_node` 4/4, with v5's own
`std/arch.dl` and `std/suppress.dl` copied byte-for-byte
(`plans/2026-07-29-comment-node-verdict.md`).

---

## 3. Open cracks, ranked by risk

**1. A real emitter defect is hiding inside a bucket label.** The scoreboard
line "2 run_error (rejection-path fixtures)" covers two fixtures that are not
the same thing. `log_retraction_rejected` is a genuine rejection fixture: the
oracle throws too. `fork_join_error_arm_is_a_value` has a **full two-tick
oracle log** (`out/fork_join_error_arm_is_a_value.oracle.jsonl`) and the
emitted module dies on `SQLITE_ERROR: malformed JSON`. That is a wrong answer
counted under a word that reads like an expected one. Shape: compound
arrivals `ok(body_one)` / `error(502)` matched in a level body, the
SLOT-TERM-STRUCT family. ARCH row `fork_join_malformed_json`, unowned.
The meta-crack is worse than the defect: one bucket name absorbed it.

**2. The flagship's parity claim is a subset claim.** `flow_edge` reads
"2184/2184 matched", which means every v6 row matched, not that the answer
matched: v5 has 2462 and the 278 v5-only rows are undiagnosed. `flow_node_type`
is **empty** on the v6 side, and an empty rel produces no diff rows to
classify, so it grades quietly. `flow_param_type` is 0 matched for a referee
key reason (v5's `root::` prefix and qualified type names against v6's bare
ones), which is rig work, not rail work. ARCH row `flow_parity_residue`.

**3. Ingest is the felt gap and it got a real number today.** The crawl bench
(`v6/tsv2/CRAWL-BENCH.md`, merge `a192cd35`, coordinator re-ran it) puts v5 at
42,739 files / 389 repos / 12.07s = **3,541 files/s** against v6 at 779 files /
8 repos / 19.15s = **40.7 files/s**. That is ~87x on the same host. Two honest
caveats in both directions: the legs are not the same work (v5 emits a scan
fact from a git tree, v6 runs cst+type+call+df extraction over a working-tree
file), and the historical v5 memory-doc number (7,244 files/s) did NOT
reproduce on this host today. Also structural: **v6 has no org fan-out
spelling at all**; the bench supplies it with a shell loop, one served process
per repo.

**4. Every full battery run can leak a hung v5 process.** `v5_lsp_exit_hang`:
`dl --lsp` answers `shutdown` and then hangs on `exit` + EOF. The audit
confirmed the mechanism: `lsp-diags.sh`'s trap kills `DRIVER_PID`, but that is
the python driver and `dl --lsp` is its child, so `kill -9` on the parent
orphans the binary that is already refusing to exit. `lsp-diags` is in
`green-all`. Three ~4-hour-old hung processes were found and killed earlier
today. This is the "nothing seizes the machine" law in the wild.

**5. Two flakes, one of them frequent.** `store` golden.test fails ~1/18 under
3x-concurrent load with **zero error payload** (a native/process-level
signature, not a JS assertion; leading unproven candidate is ~40 unclosed
in-memory clients per run). `reactor.test.ts` "file+folder coalesce" fails
6/18 under the same load, and its cause is known: a wall-clock `bufferTime`
assertion, exactly the class already killed once in v6/dl with virtual time.
A battery that flakes teaches people to re-run instead of read.

**6. Storage amplification has no sensor.** Coordinator measurement relayed at
audit time, **no committed receipt**: a ~3.4MB comment-facts database roughly
two-thirds duplicated join-key text (163KB for 56 distinct paths). The gap is
not the bytes, it is that nothing measures them: `memory-soak` asserts page
count is flat under churn, `GET /stats` reports page_count / freelist / dbstat
sums, and neither reports bytes-per-fact or duplicated-key share. A 3x storage
regression lands green today. `file_span_redesign` removes the biggest class
structurally, which is a reason to build the sensor FIRST, or the redesign
gets credited with a number nobody measured.

**7. Diagnostics have no source locations, and refusals can carry the wrong
name.** Refusal messages print `location=rule-index unavailable` because
`parse_dl` keeps no source positions. Separately, `Var = expr` (the first
thing a prolog reader types, and terra typed it in the flow rig) is
unregistered and dies as `unbound_head_var` with no mention of `=` at all.
The probe-output guard had the same shape until this week: refused under
`unbound_head_var`, root cause was goal placement. The class is "the refusal
is right, the name is wrong", and it is the cold-author experience.

**8. Two disjoint runtimes, one graded.** `v6/dl` (server, hosts, ingest)
still evaluates DELETE-all-and-rebuild through `lowerSql`; the graded
incremental engine is `v6/tsv2`, reached through the `serve/` wrap. The bridge
landed, so a served process now runs the graded engine, but the older runtime
is still there and still carries `endurance` and `leak-soak`. Nothing is wrong
today; it is a standing two-worlds cost.

**9. The compiler batch sits near a swipl GC corner.** swipl 10.0.2 aborts the
sweep with `system error: Mismatch in up phase` under `-g`, deterministically
at the 88th collection once the corpus reached 163 fixtures. Worked around
with `gc(false)` for that one-shot process (`sweep.sh:26`). The workaround is
right and free; what is open is a minimal reproducer worth reporting upstream,
and the knowledge that the corner moved as the corpus grew.

**10. Generated artifacts go stale silently, three times today.** The
staleness gate (built for exactly this) caught the extract binary predating a
merge twice and `door-handwritten.ts` once. The gate works; the frequency is
the signal.

---

## 4. Design tail

`conformance/rulings.pl` holds **47 ruled rows**. The tail is the open half:
**56 distinct `SLOT-` names** appear across `plans/` and the prolog sources.
Not all are live (lab slots close when their arc lands), but the live ones
group cleanly:

**Spelling, and now under a standing law.** The user named the process
failure this evening: the language grew unsighted syntax through agent arcs
(`:=`/`==`, the `type` keyword, the naked span pair, kwargs). Standing
consequence: no new surface spelling lands without a user decision card BEFORE
merge. Open here: SLOT-BIND-SPELLING (`:=` is not an rxjs, prolog or SQL word,
and `=` must refuse by name whichever way it goes), SLOT-TYPE-DECL-
DISTINGUISHABILITY ("types and rels are indistinguishable to a fucking human",
three candidate spellings priced), SLOT-DECL-SPELLING, SLOT-TERM-STRUCT
(prolog compounds are refused non-structs; the fixture in crack 1 is a member
of this family).

**The value plane.** SLOT-GC-TIMING (dictionaries are monotone and invisible
in the tick log, so collected and uncollected print identical bytes),
SLOT-ARRIVAL-CANONICAL-ORDER (partly discharged by the key-order ruling),
SLOT-INTERN-SCOPE, SLOT-JSON1-FATE, SLOT-SEMANTIC-DIGEST. Plus the three
`file_span` decision cards: text through a world host vs a stored content
plane; `file` as rel and type or unified; rev on the file value now or later.

**Lifecycle and arms.** SLOT-QUEUE-PACING, SLOT-ARM-ARGUMENT,
SLOT-ERROR-VARIANT-NAME, SLOT-ERROR-TERMINALITY, SLOT-COLLAPSE-CHANNEL,
SLOT-BOOT-OCCURRENCE, SLOT-RETENTION-SPELLING (consumption-arms lab), plus
SLOT-UPDATE-ARM-LEVEL-SPELLING, SLOT-DELETE-ARM-DISCRIMINATION,
SLOT-LOG-FINALIZE-REFUSAL, SLOT-ARM-SIBLING-WILDCARD (update-arm lab). These
have been priced and waiting the longest.

**Extraction.** SLOT-EXTRACTOR-WAIVER (scoped to markdown by the comment lab's
own measurement), SLOT-MARKER-CAPTURE, SLOT-COMMENT-KIND-VOCAB,
SLOT-SPAN-UNITS, SLOT-TOKEN-STRIP (all four answered by that lab, awaiting
bless), and the four doc-format slots (SLOT-KEYPATH-SPELLING, SLOT-MD-GRAMMAR,
SLOT-HTML-DIRT, SLOT-DOC-VALUE-TYPES).

**Ruled today**, so no longer tail: `bool_column_type = two_valued_column_type`
(bool becomes a real column type, strictly 2VL, overruling the golden plan's
row-presence shape as un-ergonomic) and `numeric_precision =
approved_phase5_design` (float/REAL + `avg()` gets its yes).

Also still open and older: keep(count) per-rel vs per-key, Q8 residual, the
extraction ambiguities A4/A14, and the rel-spreading verdict's six slots
(design only, not wired).

---

## 5. Distance to daily use

Read as: could the user run this on his own work tomorrow.

| capability | state | what stands between it and daily use |
|---|---|---|
| **CLI** | `bop` ships: `serve` / `run` / `check` / `load` / `q`, exit contract 0 clean / 2 named refusal / 1 broken, verified by coordinator runs | It is `node` in a repo checkout, not an installed binary. No `--changed` gating. |
| **LSP diagnostics** | Real. v5's own `dl --lsp --diag-db` reads a tsv2-served `diag_v5` table over real stdio; appear and retract both proven | **Line numbers are 0.** Blocked on `file_span_redesign` deriving line/col. Editor rendering and clean shutdown explicitly not proven, and the exit hang leaks a process per run. |
| **Watching + extraction** | Live: `fs.watch` behind a bind seam, `bufferTime(100ms)`, `git ls-files` enumeration (node_modules never walked), content-addressed re-extraction, kill -9 exactly-once | Speed (crack 3). One host decl means one record shape, so two shapes of one file extract twice. |
| **Callgraph rails** | Graded against v5 on a pinned corpus, 0 unclassified, `just flagship` in green-all | Line column dropped (same `file_span` blocker). |
| **Dataflow (the alpha's point)** | Ported and rig-graded, four queries | Crack 2: one rel empty, 278 v5-only edges undiagnosed. |
| **Comment / suppression rails** | Byte-exact vs v5 in a lab | Nothing promoted. The fixtures and both receipt programs live only in commit `9b5ba958`. |
| **Cross-repo / org scale** | v5 does 389 repos in one program | v6 has no org fan-out spelling. A shell loop is not an answer. |

The honest one-line version: the engine is ahead of v5 on semantics
(retraction, incremental joins, typed value plane, refusals) and far behind it
on throughput and on the boring surface that makes a tool usable daily
(locations, installation, org scale).

---

## 6. Lanes at write time

| lane | kind | state |
|---|---|---|
| `assign_composition_lab` | opus worktree | running. Its "slice" briefing is known-wrong on one word: slice is sub-span projection, corrected by the file-span design doc after dispatch. |
| `finish_the_job_epic` | opus worktree | running. Carries schema import, the bool/precision rulings, the decl-legibility cluster. |
| `world_health_reconcile` | opus worktree | this document and the ARCH fold. |

Landed and merged earlier today, all review-gated: crawl bench (`a192cd35`),
struct host output seam (`265da55f`), flow parity merge (`837fe7f2`) and rig
grading (`ed81cdc6`), the aggregate GROUP BY fix (`6522f848`), the comment-node
dogfood lab (`5425ed07`). Handoff record:
`chat_log/20260729.2.fable-closeout-handoff-to-sol.md`.

---

## 7. If only three things get picked up

1. **Split the run_error bucket and own `fork_join_malformed_json`.** A wrong
   answer is sitting behind a word that reads like an expected one, in the one
   gate the whole project trusts.
2. **`file_span_redesign`.** It is the only open item that closes several
   shipped honesty gaps at once: LSP line zeros, the flagship's dropped line
   column, the flow rig's hand-built concat identity, the referee's python
   coordinate translator, the comment rails' grep-for-text hosts, and the
   biggest storage duplication class. One hole, four warts.
3. **Make the battery say what good looks like.** Refresh the justfile and
   SCOREBOARD expected counts, and fix the two flakes. Every other finding in
   this document was found by reading receipts; receipts that lie about their
   own targets are how the next one gets missed.
