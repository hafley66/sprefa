# sprefa

Reactive datalog-over-code engine ("dl"), living at the **repo root** (v5 lifted
2026-07-01): SQLite-welded, facts extracted via `scan`+`regex`/`ast`/`sg`/`json`,
recursive rules lower to a SQL fixpoint. Prior iterations: v3/v4 working trees in
`~/projects/sprefa-archive-20260701` (also full git history); the OG coordinate
model (strings/refs/byte-spans) in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`README.md`**.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

**Completed-arc history** (85 landed items, full detail) lives in
`.agent/memories/sprefa-task-ledger.md` — read it on demand, not auto-loaded. This
file keeps only the standing laws + currently-open work.

## Standing laws (user-set, non-negotiable, apply to every agent at every level)

- **Doubt yourself before asserting** (user-set 2026-07-23): you are a compression
  algorithm, not an oracle; a large share of your confident claims are wrong. Hedge,
  verify against the code, and do not tell Chris what to do as if it were settled. When
  you lack enough info to answer, or he is asking outside his own expertise and needs
  more depth than you hold, SAY SO and go get it (read the code) rather than guessing.
- **Build-vs-buy**: never assert "we should write our own" for any common-shaped
  problem (queues, servers, schedulers, parsers, telemetry) without FIRST running
  library research and presenting a written analysis of the candidates and why each
  does or does not fit. No one-line dismissals of libraries. The analysis comes
  before any bespoke line of code.
- **Self-diagnosis before execution**: the daemon does not run until `dl daemon why`
  can state, from the on-disk trail alone, what it was doing and what it consumed
  (CPU, disk I/O) — including after a SIGKILL or crash. No receipt runs, no smoke
  tests that start the daemon, until that capability is installed. Never make the
  user ask "why is it slow" — the system answers that itself.
- **Nothing seizes the machine**: CPU (QoS/nice), disk I/O (IOPOL_THROTTLE), and
  thread budget are all capped in `apply_daemon_budget`. First-run rebuild included.
  A change that can beachball the machine is a blocking defect, not a follow-up.
- **The failure ledger is standing** (user-set 2026-07-18): every incident that
  bites us gets an entry in docs/failure-modes.md — incident receipt, law, rail
  status — following its "how a new rail gets born" pipeline (incident -> RCA ->
  fail-pre-fix test -> rail -> entry). No incident closes without its entry. Do
  not rely on skill self-updates to carry this knowledge; the doc is the record.
- **eprintln never comes back** (user-set 2026-07-18 PM): no `eprintln!` ever
  returns to `src/**`. Diagnostics go through `tracing` macros only; the rare
  CLI-UX line that must bypass tracing carries an explicit `@eprintln-ok`
  waiver. `.dl/no-new-eprintln.dl` ratchets the count to zero and the baseline
  never rises. Applies to every agent at every level.
- **Infra is bought, never built** (user-set 2026-07-18 PM, supersedes the
  scheduler plan's build-on-jobq verdict): scheduling, job queue, HTTP serving,
  daemon lifecycle/supervision, and logging/telemetry run on established Rust
  libraries (or the OS service manager). Logging = the `tracing` crate spine —
  new signals land as tracing events/subscribers, never a parallel bespoke
  pipeline (invlog/why/verdict are migration targets onto subscribers).
  Bespoke versions of these subsystems are migration targets, and no new
  bespoke line lands in them beyond keep-the-lights-on fixes. The datalog
  engine core (lowering, fixpoint, extraction) remains the one legitimately
  bespoke layer.

## v5 Work — open items only (landed detail: .agent/memories/sprefa-task-ledger.md)

### In flight
**v5-port + perf-tracing arc** (2026-07-27 late, plans/2026-07-27-v5-port-perf-header.md):
scopes fixtures LANDED (conformance 97 -> 109, merged d481159e); opus diff review
LANDED (plans/2026-07-27-diff-review-findings.md — finding 1 double-fire = USER
ACCEPTED no-fix, 2+3 http fixes dispatched, rest banked); SLOT-LIB filled
(tracingChannel + pino, user approved the pino dep,
plans/2026-07-27-perf-tracing-buy-verdict.md). http fixes 2+3 LANDED (mergeMap
body read + SSE response.end, 2 regression tests, dl 76/76). ghcacher phase 1
LANDED (v6/dl/fixtures/ghcacher.dl ACCEPTED by the server + ghcacher-findings.md,
9 findings F1-F9): HEADLINE = F7 engine crash, first real host response commit
dies `SQLITE_ERROR: no such column: NaN` (1_hosts.ts:491 commit path, statement
text not surfaced by LibsqlError; root cause OPEN, fix agent queued BEHIND the
P0 tracing merge since per-statement tracing surfaces the failing SQL).
PROVEN GAPS awaiting user word (the zero-new-constructs exception clause):
F2 no clock/cadence = the SLOT-SWR-defining gap (spelling A in-language chosen,
B external-cron documented); F3 no json term-extract, array-explode
inexpressible; F8 rel(1) is whole-table sweep + silently inert on rule-headed
rels, Key(text) unimplemented (feeds the Q8/Key ruling); F9 no effect_log rel
(self-diagnosis law gap). F4 confirmed the not_stratified guard fires correctly
on the v5 etag idiom. P0 tracing spine LANDED (0_trace.ts: tracingChannel +
pino, DL_PERF_LOG opt-in, one JSONL line/tick, overhead -0.02% within noise,
dl 79/79; ratchet filter tightened to Channel\.subscribe call shape; seam gap
recorded in 0_trace.ts header: EDB-plane writes bypass SqlRunner via hand-
rolled execute$). FIRST PARITY NUMBER, ugly and now visible: ingest_corpus
over 251 rxjs .ts files = ~103s (~2.4 files/s) vs v5's 7,244 files/s; the
harness's per-file rt.rows() full-table read is superlinear and suspect, but
extract_ms is only ~21ms/file so engine-side cost dominates — the perf JSONL
now exists to decompose this. DECOMPOSED (overnight, 60-file pinned corpus,
DL_PERF_LOG): wall 4977ms = engine ticks 366ms (~6ms/file, 19 stmts/tick,
growing 4.4 -> 9.4ms as tables fill) + extract stream 438ms + subprocess
spawn ~0.6-1s + ~3s UNATTRIBUTED inside ingestFile (toFactLines, span_line
byte scan, diff reads — needs finer spans, wait for F7 merge since that
agent owns 4_ingest.ts). ALSO: DL_EXTRACT_BIN default is a DEBUG build in
the stale extract-golden-plan worktree (4_ingest.ts:93) — the banked
hardcoded-path soft spot is now a measured perf item; a release build +
in-tree path is the obvious first win. Endurance re-proven 3/3 on the
merged main tree (PORT=17311). TSV2 PHASE A LANDED (v6/tsv2: IGenProgram seam,
generic tickLoop via rxjs expand, 2 hand-carved gen files BYTE-IDENTICAL to
the prolog oracle incl a perturbed schedule, import gate green, 6/6 tests,
conformance 109; emitter-spec margins recorded in the agent report: keyed()
inert on raw arrivals vs live on edge heads, TEXT-collapse + LIKE compound
matching, one multiset-diff covers log+set, carryPending simplification
FINDING 3 in switch gen file). F7 CLOSED (merged, dl 83/83): root cause = multi-line sh output parsed
row-per-line instead of line-per-column, tag text through Number() = NaN,
typeof-guard passed NaN, bare NaN spliced into VALUES = "no such column:
NaN". Fixed: line-per-column parse when line count matches output-column
count, Number.isFinite rejection naming rel+column pre-SQL, execute$ errors
now carry the statement text (self-diagnosis gap closed), 4 fail-pre-fix
regression tests, failure-modes class 36 filed. Post-fix ghcacher marble:
resp/stars/full_name/change_log all land, stream alive.
PERF ARC RESULTS (overnight): attribution sub-spans landed
(read/extract_wall/fact/diff/commit); diff_ms was 77-81% of wall and O(n^2)
(unscoped SELECT through the correlated-subquery decode view, JS path
filter); FIXED via rowsForPath (WHERE on the interned path id against the
UNIQUE(path,...) index; promoted onto IDlRuntime by the coordinator, no
instanceof fallback). Receipts: diff_ms 3676 -> 16ms flat, 13.3 -> 74.2
files/s with the release extract bin (overall 2.4 -> 74.2 across the arc).
NEXT DOMINANT COST: commit_ms ~10.8ms/file (the store commit path), next
perf target, unassigned. v5 yardstick 7,244 files/s, distance now ~98x.
TSV2 RECONCILIATION: phase B merged (target-neutral plan term per the rust
directive, SQL-check 8/8 vs fixture expectations, 15/15 plunit); first
cross-run caught 4 emitted seam-shape misses (the clean typecheck was
vacuous until run-emitted.ts imported the drafts; gen_emitted/ now
quarantined from the type graph, drafts load via computed dynamic import,
package green). ROUNDS 2+3 LANDED, RECONCILIATION COMPLETE: emitted modules
on the A runtime are byte-identical to the prolog oracle on ALL THREE runs
(both fixtures + perturbed schedule), independently re-verified by the
coordinator. Round 2's finding: the tick-number dependency did not survive
the real seam (plan term converged to snapshot-diff deltas +
arrival-projected upserts). Round 3: compound columns render canonical term
text at read via CASE json_valid+json_type in lower.pl SQL (shared with the
future rust backend). gen_emitted back in the type graph. PHASE C SWEEP LANDED
(v6/prolog/compile/SCOREBOARD.md, regenerable via v6/tsv2/scripts/sweep.sh):
109 fixtures = 92 UNSUPPORTED (named constructs) / 9 IDENTICAL / 8 WRONG.
Ranked backlog: unmarked edge trigger 48 (needs real backlog-replay design,
not a quick widen) > comparison ops 12 > only+guard 9 > aggregates 9 >
arithmetic bind 5 > json destructure 4. All 8 WRONGs diagnosed: 5 = the
TEXT-only column model loses int/string distinction ("1" vs 1, structural);
1 = compound arrival text vs json1 match mismatch; 2 = rejection-semantics
fixtures with no comparable log. Sweep also fixed 4 real compiler bugs
(declared_refs union, multi-clause head DELETE wipe, @libsql number->REAL
integer corruption via bigint binds) and added a safety gate that caught 3
silently-miscompiled "identical" results (comparison/bind/head-arith now
refuse instead). Open: retention keep(count) not lowered AND invisible to
tick-log-only grading (needs final-state in the grade); 4 empty-schedule
fixtures pass vacuously. MORNING DESIGN CALLS: column typing (int columns
in storage vs TEXT-only), unmarked-trigger semantics, final-state grading
leg. PHASE D PARSER LANDED (2026-07-28, merge 10053236, coordinator re-ran
roundtrip.sh + conformance in worktree AND on merged tree): parse_dl.pl
DCG + print_dl.pl + dl_view/ (all 109 fixtures rendered as .dl text) +
SYNTAX.md + roundtrip.sh. G1 109/109 variant round-trip; G2 ghcacher.dl
7 decls/9 rules with 8 named gaps (host_decl/probe/query have no term-form
shape), conformance.dl 23/28 zero findings; G3 conformance untouched.
Central spelling call recorded in SYNTAX.md: bare identifier = variable,
atom constants single-quoted (supersedes dl.langium per the stopgap
ruling). Decl spelling: `rel name(cols) [log|set] [keep(..)] [key(..)]`.
NOT wired into compile_fixture yet — held behind the pending
latest/combine/zip + Key-decomposition words since they change spellings.
Hosts half of D still queued. ARCH.pl made current same morning (a6c1225a: tsv2 algorithm rows,
js_conformance_leg flipped done via the sweep, in-flight task rows;
go 7/7, atlas re-emitted).

**v5 BACKGROUND OPS (overnight 2026-07-27, user asleep)**: daemon swapped to
current binary (~/.cargo/bin/dl restored from target/release, was missing —
plist pointed at nothing while dl.old-1301 held the socket since Sunday);
launchd plist gained EnvironmentVariables PATH (homebrew+cargo) because every
sh effect exit-127'd under launchd's bare PATH — the doc-gen trigger then
fired and its output is committed (f76b7c10). Roots watched: sprefa (.dl/*.dl
rails + flow-interproc loaded), smashy, instant. CROSS-REPO IS LIVE:
~/orgs/.dl/{go-deps,xrepo-rev}.dl run against SPREFA_CONFIG=~/orgs/
all.config.toml (800 repos) settles in 3 ticks with real fan-in/rev-fan rows
(79 hubs). MORNING DECISION: the daemon runs the safe selfv5-only global
config; watching the orgs root persistently needs either a daemon-level
SPREFA_CONFIG (puts 800 repos under EVERY wildcard rail — the safe-default
comment warns against exactly this), a per-root config feature, or a cron
one-shot. Health also showed: sprefa root db regrew to 4.3GB (lazy-rel-tier
decision pending), 4 orphan roots incl one minted TODAY (class-14 rail may
have a gap — worth a look).
CLEANUP AUDIT LANDED (2026-07-28, opus, plans/2026-07-28-cleanup-audit-
findings.md): 24 tests audited, 11 sabotage probes, 0 removed; 7 mechanical
fixes merged (one-subscribe wildcard + zero-floor, import gate comment-blind
+ gen_emitted + direct-@libsql refusal, dead helper, 5 stale comments).
DEFECT WAVE FIXED 2026-07-28 PM (sonnet agent, 5 commits, coordinator
re-verified all suites + endurance): F2 commit() now REJECTS on tick-
pipeline fault (CommitSettlement union on reportsSubject, fail-pre-fix
5s-timeout test red->green ~2ms); F1 rowsForPath guarded by a rawSql
trace-seam test (sabotage receipt in test header: unscoped SELECT caught);
F4 bind channel + PerfTickLine.binds asserted; F8 fixture
log_stacks_within_tick_and_across_ticks added (corpus now 110; oracle-
verified; multisetDiff sabotage flips 2 fixtures red; sweep 31 compiled /
28 identical; roundtrip 110/110); labs/ DELETED from store (nothing
imported it; prolog_emit_bench.ts moved to src/bench/, swi_emit.sh
repointed; store 89->74 tests, last copy at 5d6f8fc5). dl now 92/92.
F3 CLOSED 2026-07-28 PM (merge 656694f1, coordinator re-verified 93/93 +
ratchet + endurance): BindConfig.scheduler (SchedulerLike, asyncScheduler
default, prod byte-identical), the only wall-clock rx source was
1_binds.ts interval(); both teardown tests rewritten on TestScheduler
asserting scheduler.actions.length (row equality could not discriminate a
leaked timer re-committing an identical bucket row — that WAS the false
positive), sabotage receipts red->green recorded in test headers; ~14s of
real test sleep removed. Two tests deliberately stay real-time (bucketFor
reads Date.now for VALUES; virtual firings inside one wall second would
collapse buckets). Endurance-as-gate still
ungated (open). COUNT-TEST LAW EXECUTED (cherry-pick 53762d1c -> main,
dl 96/96): EXPLAIN QUERY PLAN SEARCH-not-SCAN on rowsForPath's real
captured statement, only-requested-rows at 50 paths, and statements-per-
file exactly spineDeclsLocal.length flat across 5 vs 20 file corpora;
sabotage receipts in both test headers. PROCESS DEFECT, recurring: agent
worktrees are being cut from stale bases (three today: parser hook
failure, scheduler 244-behind, count-test 450-behind with NO v6/ — that
agent tunneled around a permission denial via git archive|tar to
materialize main's tree; disclosed, content read-only, branch history NOT
merged, test commit cherry-picked instead). Worktree-base staleness needs
a look before the next dispatch wave. CODEX LANE REOPENED (user,
2026-07-28 PM; pattern = claude-research/commands/codex-delegate.md,
OpenAI limits effectively free): first run LANDED (merge a48ed3f3,
gpt-5.6-luna, review-gated -- coordinator re-ran sweep 31/28,
conformance 110, roundtrip, plunit 17/17 on the branch): INTEGER columns
drop the json CASE wrapper via canonical_column_expr/3 int/text split;
generated SELECTs simplify to plain columns. Codex worktree removed,
branch deleted. Luna-ready brief queue: endurance-as-gate, lower/types.ts
I-prefix renames, v5 rails.dl descriptive names.
SPELLING MIGRATION WAVE 1 LANDED (merge f96c6229, codex luna, review-
gated: conformance 110, roundtrip ALL PASS, sweep 31/28 unchanged, grep
zero only(/departed( -- all re-run by coordinator): only() INVERTED into
bare-trigger + latest() sampling, departed -> finalize, combine/next
sugar, zip + unsubscribe/complete/subscribe/error reserved with named
refusals. 49 files. Key decomposition still split out (semantics arc).
CONSUMPTION+ARMS LAB LANDED (merge 445c345d, lab-death ee6bc71e, lab
commit 82bd12a8, verdict plans/2026-07-28-consumption-arms-verdict.md;
90 PASS re-run by coordinator, 7 rounds to fixpoint, 28 assertions):
consumption needs NO construct -- switch and queue are the SAME rel
under two key decls; all six observer words ground to shipped kernel
forms (subscribe/unsubscribe = next/finalize on the demand rel,
complete = finalize on the scope rel); every arm is row granularity.
Pacing: (b) one-per-drain-tick is the only spelling that implements a
queue -- (a) N-same-tick loses N-1 items at any keyed consumer and the
survivor is picked by term order of the ready view; (b)'s cost is the
drain cap becoming a queue-length cap (hard-fail at exactly 99/100
under error-at-cap). Crash-restart from the durable pending rel alone
yields ZERO ticks (durable rows do not make a firing durable --
SLOT-BOOT-OCCURRENCE, collides with the no-boot-replay endurance
goal). Error arm: reading (A) only (enum-variant destructure over a
Log envelope; second-channel refused on three grounds); on a KEYED
envelope an error arriving with a later ok row same-tick is replaced
before any arm sees it -- scheduler-batching-dependent observability,
the ruled collapse trace is the only record of the drop. Channel:
log + keyed cursor + min composes N-readers; keep(count(N)) is
tick-log BYTE-IDENTICAL to keep(all) while permanently stalling a
lagging reader (invisible to tick-log-only grading, same class as the
retention-grading gap). Desugar of signed edges: exact on plus,
inexpressible on minus (no retracting edge head). Retention slot
priced 4 ways, smallest honest = retention as an ordinary retracting
rule over the log (SLOT-RETENTION-SPELLING s1; cost = lifting
retract_from_log; rxjs has the same gap). 3 prospective fixture/5
terms graded by the real harness, recoverable at
82bd12a8:v6/prolog/labs/consumption_arms/fixtures.pl. AWAITING USER:
SLOT-QUEUE-PACING, SLOT-ARM-ARGUMENT, SLOT-ERROR-VARIANT-NAME,
SLOT-ERROR-TERMINALITY, SLOT-RETENTION-SPELLING, SLOT-COLLAPSE-CHANNEL,
SLOT-BOOT-OCCURRENCE.
EMITTER P0 LAB LANDED (merge 36977cb5, lab-death fcb47777, lab commit
53fa1f54, verdict plans/2026-07-28-emitter-p0-lab-verdict.md; user
unblocked sequencing before scale-bench landing; arc header =
plans/2026-07-28-incremental-sql-emitter-header.md + tempering): 4
statement families graded inline vs one-shared-helper on 4 fixtures,
12/12 tick-log byte identity, ZERO delta-side scans (EXPLAIN receipts
in the verdict). VERDICTS: semi-naive delta join MIXED (34 vs 7
lines), count-IVM support HELPER (42 vs 7), DISTINCT placement MIXED,
boundary-diff-from-delta-stream HELPER (16 vs 6, zero full-table
snapshots execute). Statement counts flat per tick, no arrival-row
loops. CRACKS: CURRENT_COMPILER_GATE (compiler refuses recursive/
departure/derived-edge fixtures; P0 emitted fixture-specific modules
graded through ScratchStore+TickFold); COUNT_CYCLE_RESEED (departure
coverage acyclic; cyclic P3 rides the retraction verdict's reseed);
CARTESIAN_CURRENT_SCAN (fork fixture has no equality predicate;
current side scans). P1-P4 now have proven shapes.
EMITTER P1 LANDED same sitting (merge on main after fcb47777, codex
sol no-commit flow, coordinator verified EVERYTHING on the branch:
sweep 31/28/0 per-fixture unchanged BOTH modes, conformance 110,
plunit 18/18, roundtrip, tsv2 6/6, import gate, tsgo clean):
incremental delta-join emitter is the DEFAULT for non-recursive level
rules; tick log computed FROM the delta stream (host_residency ruling
satisfied on the incremental path); SPREFA_TSV2_EMITTER_MODE=naive =
the snapshot referee; automatic naive fallback on retraction ticks,
negative bodies, edge+level mixed rels (P2/P3 scope). SCALE: s1/100k
177s -> 2.1s (84x), s2/100k 183s -> 1.1s (165x), ms/1k-arrivals FLAT
(s2: 15.8/11.7/11.1), s3/1k naive-OOM -> 6.5s under 512MB, statement
counts flat, all delta-side reads indexed SEARCH (p1-receipts.jsonl).
OBSERVABILITY-COST EXPERIMENT (coordinator scratch, DL_NO_OBS flag,
reverted, receipts in the merge message): s2/10k 1894 -> 199ms
(~=v1's 195ms), s2/100k 183s -> 10.2s naive-minus-snapshots (beats
v1's 17s) -- the WHOLE 10x-vs-v1 gap was boundary snapshot reads, v1
never paid the tick-log obligation (v1_scale_bench.ts emits no delta
log; asymmetry now noted here). P1 makes the obligation O(delta). PROCESS FINDING (codex lane): the codex sandbox cannot
write git metadata for coordinator-cut worktrees (.git/worktrees/*
is outside its writable roots) -- sol STOPPED AND REPORTED per the
dispatch law twice (ff-only, then git add); coordinator verified
(lab exit 0, 12/12 identical, conformance 110 on branch, tsc clean)
and committed the work itself. Older codex worktrees (unify, scale)
committed fine; difference unexplained, check before next codex
dispatch.
TYPES ROUND 2 LANDED (merge b47d3c00, codex SOL, lab deleted, last copy
20520177; verdict + plans/2026-07-28-types-as-rels-iteration-journal.md
are the record): fixpoint in 4 rounds (36->46->56->66 PASS + zero-finding
replay, re-run by coordinator). ENTITY and VALUE both first-class, NO
implicit default (missing policy = named failure -- user's not-sold
instinct upheld): value = content hash identity + dense-int mate +
immutable + support GC + set merge; entity = extrinsic id + mutable row
w/ immutable history + explicit checked retirement + keyed merge +
CYCLES PERMITTED (amends the round-1 cycle crack). Surrogate mate
VALIDATED: semantic hash and dense storage key are separate columns,
parent hashes consume child SEMANTIC hashes; resolves the dense-ints-vs-
content-ids ruling collision. Coexistence ranked hybrid > decl-word >
use-site (worked example in all three in the verdict). Support GC
complete ONLY on the value DAG; entity plane pays explicit retirement.
RULED 2026-07-29 (rulings.pl tail): decl_column_spelling =
colon_typed_ordered_columns (rel name(col: type, ...), source order
significant, Key(text) wrappers dead); enum_decl_in_rel = semicolon
variants in-decl; no_policy_suffix_words (set REMOVED, bare rel = set
table per engine.pl fallback, log = the only kind word, plane carried
by key(...) + id binds per the verdict's own optional-sugar note;
entity extras still need a future non-suffix spelling). Types arc has left design.
SPELLING WAVE 2 LANDED (merge aadede88, codex luna no-commit flow,
coordinator verified: conformance 112 (+2 fixtures), roundtrip G1
112/112, sweep 32/29/0 existing movement zero, plunit 18/18, tsv2
6/6, gate): 53 kind(Ref,set) entries deleted, `set` = named refusal
removed_word(set), colon types live (col_type(Ref, Column, Type)
term form, decl type is authority over C2a inference, contradiction
= decl_type_conflicts_witness), 2 new fixtures. ENUM ARC LANDED
(merge after 61817999, codex sol no-commit flow, coordinator verified:
conformance 115 (+3), roundtrip G1 115/115, sweep 34/31/0 existing
movement zero, plunit 21/21, tsv2 6/6, gate): semicolon variant decls
retained as sugar in term form (enum_decl/2), ONE shared expansion
expand_enum_program/2 (v6/prolog/0_enum_expand.pl) consumed by BOTH
the oracle engine and compile.pl -- variants become typed variant rels
(body_page/body_redirect) + derived body_tag view, reference columns
INTEGER, collision refusal named. SCOREBOARD.md noted stale (110-era)
by the agent, regen rides the next sweep-touching arc. P2+P3 HEADER SEEDED
(plans/2026-07-29-emitter-p2-p3-header.md): the 1M-competition entry
-- recursive strata across ticks, retraction as emitted support-count
SQL with the MANDATORY cycle guard, graded through the EXISTING
bench/engines rig against the PERF-REPORT.md standings (sqlite-count
class 443ms @960k is the target). Sequenced behind the enum arc.
RULED same night: rel_default_policy = value_unkeyed (bare rel = table =
replay subject); enum_variant_separator = prolog semicolon; enum storage
= N variant rels + derived tag view (lab (b), user walked through the
rust-single-table rejection). NAMED GAP from the channel thread:
retention driven by a derived rel (keep-until min(consumed.ordinal), the
Kafka low-watermark) is the ONE missing construct between log and
channel-with-N-readers; log+min-ordinal+consumed rows otherwise compose
it today. RETRACTION LAB LANDED (merge 2ef54e6e, lab-death a89acd3a,
lab commit 36980bf8, verdict plans/2026-07-28-sqlite-retraction-
verdict.md, 20/20 matrix re-run by coordinator against real sqlite3):
fk_cascade WRONG on shared children (kills child with live second
parent + dangling refs) and HARD-FAILS past sqlite trigger_depth 1000
(unraisable on this build; 1001-node chain = statement rejected);
support_count WRONG on cycles (counts never reach zero, both rows
survive a full release) and 9999 rounds/19s on a 10k chain;
recursive-CTE fixpoint reseed CORRECT on chain/shared/cycle/diamond
incl deferred-FK circular inserts, 8ms at 10k, no depth ceiling.
Crash-mid-cascade recovers both ways (ROLLBACK sim + real SIGKILL).
Confirms types-lab finding 6 (never emit FK cascade); reseed is the
retraction strategy going forward. REGISTRY LANDED (merge f414826f,
codex sol, review-gated -- coordinator re-ran conformance 110,
roundtrip ALL PASS, plunit 17/17, tsv2 6/6 + import gate, sweep
31/28/0 with SCOREBOARD byte-identical): surface/5 construct registry
(registry.pl) now drives analyze dispatch, refusal-by-absence
(unsupported_construct thrown for any functor without a live row),
parse/print body-word inventory, and a GENERATED SYNTAX.md construct
table (1_emit_registry_docs.pl). Bidirectional single-DCG stretch NOT
taken (variable-binding recovery + printer fidelity non-mechanical);
two files consult one table. One-row demo receipt (fake_reserved) in
task bmo2zn70a output. SCALE BENCH LANDED (merge 4dfac09c, codex luna two-phase incl
amendment-2 resume, review-gated -- coordinator re-ran conformance
110, tsv2 6/6, import gate on the branch; results v6/tsv2/SCALE.md,
brief plans/2026-07-28-codex-scale-bench-brief.md): 9-cell matrix
BOTH engines. HEADLINES: (1) tsv2 curve is superlinear as tables fill
(s1 ms/1k-arrivals 65 -> 196 -> 1771); (2) v1 evalProgramSql is ~10x
FASTER at every s2 size (17.1s vs 183.1s at 100k, 227MB vs 682MB RSS)
on the SAME recompute-per-tick class -- the gap is A-runtime overhead,
not algorithm; (3) tsv2 OOMs on s3 (2-atom combine cross join) even at
1k rows where v1 completes in 2.95s (1M-row result), v1 times out at
10k+ (shape is quadratic, but tsv2's memory blowup at 1k is its own
defect, unowned); (4) v1 s1 N/A-with-reason (no keyed-replace edge
semantics). Oracle cross-check byte-identical (sha 70c519e8). The
before-curve for the emitter arc now exists; P1 also owes the s3
memory answer. PROCESS: luna's first landing predated amendment 2;
resumed by session id per codex-delegate.md, model re-pinned. STORE-ADOPTION FINDINGS LANDED
(plans/2026-07-28-store-adoption-findings.md, sonnet, merged): PREMISE
CORRECTED -- the js store's cascade/reconcile is a generic liveness
propagator over (tag,id) keys + cx_dep edges, NO joins; the actual
derivation engine 3_runtime rides is lowerSql's DatalogEvaluator which
does DELETE-all + rebuild per tick and lodash differenceWith diffing,
the SAME naive shape as tsv2. The count-IVM-beat-DRed-4-5x receipt is
the RUST store only (engine.ts header says so itself). Consequence: no
js-store adoption win exists; the real tsv2 perf path is an incremental
join engine (rust store port or new strategy), to be motivated by the
scale bench curve. Prototype correctly declined with evidence.
TYPES-AS-RELS LAB LANDED (merge 7a416fac, 36 PASS, lab deleted, last copy
b58d1ece, verdict = plans/2026-07-28-types-as-rels-verdict.md): hypothesis
HOLDS on the value plane -- one construct (rel), struct = rel+set with
key(every content column), id = content_id() stdlib bind, enum = N variant
rels + DERIVED tag view, list = fixed-arity cons cells (amendment 1,
souffle made the same call), policy bundle = FOUR bits (identity/mutation/
lifetime/MERGE, amendment 2). THE CRACK: cycles -- content ids cannot
express cyclic graphs (parent id derives from child ids); cyclic needs
extrinsic keys where support counting stops being a complete collector.
DOMINATION DISSOLVES into support counting, complete because interned
graphs are DAGs by construction; graded: shared child survives (support
2->1), last release cascades 5 rows one tick; SQL ON DELETE CASCADE on the
same store deletes the shared child + leaves dangling refs = decisively
wrong + no rx lowering (finding 6) -- FK cascade must NOT be emitted.
Spellings priced (b) prolog functors > (c) plain rels > (a) json braces,
criteria visible, no fiat. Slots: OWNERSHIP-MARK = no mark on value plane;
ENUM-SHAPE = variant rels + derived tag; INTERN-SCOPE = per type;
JSON1-FATE = untyped json only never cache. Souffle verified (RecordTable
flyweight, monotonic = no GC precedent; bit-packing recollection REFUTED,
split is by field count). Top ambiguities: dense-ints-vs-content-ids
RULING CONFLICT (two standing rulings collide); tick log must print
VALUES not ids or migration/grading break; dictionary rels appear in
boundary deltas. ALL AWAITING USER RULINGS alongside the match-lab set. MATCH FRONTIER LAB LANDED 2026-07-28 PM (merge aeba1b72, 63 PASS, lab
deleted per protocol, last copy 5ba7b0c5, verdict =
plans/2026-07-28-match-frontier-lab-verdict.md): event axis HOLDS, four
cracks: Ta (DISSOLVES into pending rel, confirmed by tick-log diff both
ways — primitive Ta's log depends on an engine delivery choice, encoding
has no knob; rides the any-body-atom ruling), flagship transition rule
(C2 crack: loses N-1 of N intra-tick transitions, count depends on
scheduler batching), not() in +> arms (unstratified + arrival-order
dependent, silently), lifecycle arms over Log rels (statically dead,
retention prunes with no delta). C7 = REAL ENGINE DEFECT beyond design:
the Ti carry set is not durable in either implementation, crash loses
pending firings (endurance-law violation, unassigned). Slots: SPILL =
error-at-cap never spill; TA-MARK = no marker; NEST = not forced;
LEVEL-ARMS refuted-as-posed (engine already refuses; real restriction is
one-rel-one-rule-kind on heads); COMPLETE candidate = finalize(scope_row)
w/ groupBy duration selector; new open: SUGAR-SCOPE, UPDATE-ARM. Syntax
rec ordering: (1) Ta spells nothing; (2) SQL trigger family
inserted/deleted + OLD/NEW beats next/finalize (AFTER UPDATE gives both
in one body, kills the two-arm cut question); (3) drop mirrored -> (taken
twice, silent term-form absorption, conflicts q8 ruling), keep <-;
(4) +> optional sugar; (5) block word partition/groupBy over match;
(6) never => or | in term form. Rx directness 24 DIRECT / 1 vacuous /
7 ENCODED / 2 IMPOSSIBLE (Tn occurrences; incremental min/max over
retractable set). ALL AWAITING USER RULINGS.
gen-index.sh now excludes node_modules (INDEX.md was flip-flopping 1714 lines).
ARCH covers/2 rows for scopes.pl landed (departure_form fixture-covered,
uncovered 10 -> 9, map re-emitted). failure-modes class 35 filed (dangling dev
servers; stdin-watch rail proposed, awaiting word).

(v5 side: none. The 2026-07-19 AM wave is CONFIRMED LANDED on main, verified 2026-07-27:
src/eventlog.rs event trail + `dl daemon events`; `dl daemon health`
(src/cli/health.rs); class-14 rail (`hook::refuse_worktree_cold_check` +
tests/it/worktree_cold_check.rs); storage diet (a). `next` is 0 ahead / 244
behind main — nothing lives there. The 2026-07-18 wave landed in full earlier.
Detail for both: .agent/memories/sprefa-task-ledger.md. Receipts still live
from that wave: named_call_site is 61MB serving one join each,
inline-vs-keep = user call; .dl/rails.dl:62-64 still uses `p`/`l` and owes the
descriptive-name rename.))

### Blocked on user word
- [ ] **drop the orphaned `rel_port_of_reach` table + VACUUM** (one rewrite, not two): daemon stopped, `DROP VIEW IF EXISTS rel_port_of_reach_txt; DROP TABLE IF EXISTS rel_port_of_reach; VACUUM;` against `~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite`. Table 7.6MB + its PK autoindex 8.6MB = 15.5MB reclaimed; the deleted rule leaves the table behind.
- [ ] **rm the 3 overnight orphan roots** (~1.86GB, minted by agent-worktree pre-commit hooks before the class-14 rail existed): `cd ~/.local/state/sprefa/roots && rm -rf 5658fb5a59d0f252 c22f2b330d2dd1f7 ea3041acfc1af14c`. `dl daemon health` prints this exact line now.
- [ ] **lazy rel tier decisions** (plans/2026-07-19-lazy-rel-tier.md): syntax (`rel lazy foo(...)` vs `@lazy`), opt-in vs health-suggested, and whether demand-materialize-with-eviction is wanted at all or VIEW-only suffices (VIEW-only = zero new deps, zero policy code). Context: post-VACUUM the root db regrew 814 -> 877MB in hours with freelist ~0 (new pages, not churn); the 39x db/corpus ratio is the standing defect this decides.
- [ ] **filesize-rail ruling**: verify.sh exits 2 — 29 src files >500 lines are NOT in scripts/filesize-allow.txt (all already over budget at pushed main a3c09e3f, none crossed this session). Grandfather (allowlist + .dl/file-size.dl rows, shrink-only law) or schedule splits.
- [ ] **instant dom-match.dl rewrite** (user-side repo): drop pull/matches_latest/matches_body + both bucket columns onto `matches_resp(body) <- @async clock(5, _), matches() -> (body).` — caveat: matches_resp then accumulates distinct bodies unordered; keep a bound bucket if strict latest-wins matters.
- [ ] **worktree removal** (refreshed 2026-07-27, supersedes the 2026-07-19 row which undercounted by 40): reconcile pass found 42 worktrees. 34 are fully merged into main; all their uncommitted work is banked as 13 patches in archive/worktree-salvage-2026-07-27/ (README has per-patch inventory). `git worktree remove` was permission-blocked for the agent — the exact removal + merged-branch-deletion commands are in that README, run them. 8 unmerged trees stay alive (lsp-diags ahead 12, types, codex-intern, codex-qscip, g4-unify, refactor/file-splits ahead 7, vscode-flow-panel, extract-golden-plan RESOLVED: user merged it themselves (a85c9a70, 2026-07-28 PM) and checked out main; the session now rides main directly (cleanup/2026-07-27-reconcile is fully contained in main and stale). The DEBUG extract-bin default at 4_ingest.ts:93 now points at a merged tree, still a perf item).

### Next up (dispatchable, not started)
- [ ] **storage-diet 4a**: WITHOUT ROWID junctions; then A=1a dense dictionary ids; step 5 coordinate-composite elimination rides ref-spine. Direction 5 CLOSED 2026-07-19 (branch index-audit dc9b67b1: planner-honest demand filters in create_auto_indexes — PK-prefix on rowid tables, tiny-rel floor, constant-column; 771 -> 262 idx_, -117.7MB dbstat on the root snapshot; two policies measured-and-rejected with receipts: broad low-selectivity loses to value skew, PK-prefix on WITHOUT ROWID flips fixpoint join sides).
- [ ] **erase public no-daemon split** (user directive 2026-07-18): one server code path, `--no-daemon` internal-only; erases the two-db-worlds split. Big it-suite touch — schedule alone. Now also owns failure-modes class 23 (a one-shot positional under a daemon-served root silently returns the watched program set's results — `run_file_via_daemon` sends only `{"root"}`).
- [ ] **scheduler execution steps 1-2** (scope rows + readiness; shard = schedulable unit for every family, perf-fed costs, demand join as rows — d13dcf56). Write-volume budget lever lands here.
- [ ] **class 18 residuals**: ~~sg/ast_yaml internal ast-grep tree not shared with AstTreeCache~~ CLOSED 2026-07-19 (branch ast-tree-share: per-file SgRootCache embedded in AstTreeCache); ~~daemon-side req_id mid-tick cancellation~~ CLOSED 2026-07-19 (branch reqid-midtick 9ddf1280: run_job re-enters the causing request's reqid scope, cancel probe at component boundaries, abort-consistency test) — class 18 fully closed.

### Parked (wake on demand; plans exist)
- Auto-architect umbrella (docs/vision-auto-architect.md); decomposition + resource-scheduler children written, unexecuted.
- ~~Auto-refactor residuals~~ CLOSED 2026-07-18 (branch auto-refactor): audit found both "residuals" (brace-head rewrite, physical move + mod surgery) landed 2026-06-12 (#17, f859585e); this arc added the last gap, statement-level regroup when a brace leaf's rewrite exits its head. Audit table in plans/2026-05-31.
- vscode Wave 4; LSP thin client; turnkey query surface (`dl q`, verbs, MCP tools); measures top-K views; deck-graph sym-key migration.
- Change-cost friction inventory (plans/2026-07-10-change-cost-friction-inventory.md); ambient-config hermeticity top.
- Kimi trio prompts (reading-order/lib-taint/session-compile) — worktrees stale off old next; recut or delete.
- Low: 159-changed-paths mystery; tick_root pairing residual (c33ffc04).

### v6 STANDING PLAN (user-set 2026-07-25, execute IN ORDER, do not improvise past it)
1. ~~Restore green + commit~~ DONE (verified 2026-07-27: store 89/89, dl 74/74, both
   typechecks clean, `src/lib/rxjs.ts` orphan gone). Every green state gets a commit,
   standing. NOTE for item 2: the restored `sequence` helper still sits in
   engine.ts:115 with 2 call sites (:743, :744) — it is the first thing item 2 deletes.
2. ~~Undo rxjs over sync code~~ DONE 2026-07-27 PM (agent arc, merged): `sequence`,
   `run_then` (both copies), `execBatch`, `run$`, `inOrder` deleted; sequential run is
   `concat(...).pipe(toArray())` inline; rowsAffected flows through SqlRunner/batch/
   cascade/reconcile/TemporalStore/runAll; side-effect maps became `tap`; sync unwraps
   (`from(rows)->map->toArray`, rxjs `groupBy` over in-memory keys) are plain array
   code. Legitimate voids kept with reasons: `executeMultiple` (driver resolves
   nothing), rollback-path `catchError` swallows. Receipts: store 89/89, dl 74/74,
   both typechecks, ratchet 3, goal-endurance 3/3, statement counts unchanged.
3. ~~Single subscribe point~~ DONE 2026-07-27 late PM (agent arc, merged): ratchet
   reads 1, baseline lowered to 1. `serveDl(cfg): Observable<DlAppEvent>` in 6_http.ts
   IS the app, cold; main.ts's one `.subscribe` starts it. Program swap =
   `switchMap` on accepted loads only (bad program -> 400, running program survives);
   SSE clients are inners with `takeUntil(socket close)`; HostRunner lost
   start/dispose/Subscription for one cold `effects$` (boot replay under `defer`,
   semantics unchanged); `DlRuntime.commit()` now throws instead of hanging when the
   loop isn't running. Receipts: store 89/89, dl 74/74, ratchet 1, endurance 3/3,
   golden curl-session PASS, no Subscription fields. Honest residue: the
   `commits$`/`reportsSubject` Subject pair remains (not a collapse blocker, still
   the open item against the no-Subject-bridge corollary); `server.close`/`readBody`
   Promise wrappers remain (the Promise-above-the-seam arc); `tasks.d.ts:128` names
   `StartServer` in a past-tense M10 record (renamed to `ServeDl` in 0_types.ts).
   One golden flake in 1/10 runs under heavy parallel load, not reproducible,
   recorded in the agent report.
4. **Rxjs rule of engagement**: before writing ANY new rxjs, stop and ask the user
   first: is this making sense, is there a shorter/more direct way, fewer variables,
   fewer methods. No new operator chains land without that check.

### v6 primed queue (user, 2026-07-27 PM, unordered — "i want a lot of things")
- diags done + LSP hosted from TS (best-buy research first; note: v5 `dl --lsp
  --diag-db` boots NO engine and polls `diag_v5`, which 5_diag.ts already creates —
  the zero-code interim is pointing v5 at the v6 db).
- endurance goal: v6/dl/scripts/goal-endurance.sh IS the end-goal definition
  (kill -9 mid-delay, reboot, value lands exactly once). Phase 0 green; phase 1 =
  the pending-witness wedge + no boot replay of unanswered demand.
- snippets proving each v5 builtin rel's v6 behavior, ZERO new language features.
- bootstrap story: how the language owns its own utilities (swipl-to-C analogy);
  rust return eventually (souffle-of-rust + rx logic); formalizing the v8 event loop.
- self-diags on our own .pl files (pick up by pattern/extension/marker word).
- generic `--changed` concept (biome-style recent-change-lines gating) directable
  from dl; the old pre-commit hooks did this.
- graph-algo library in sprefa-store (user 2026-07-28: "very high source of
  non squared algos ... for complex graph algos either sqlite or ts if
  needed at runtime"): recursive-CTE and/or ts homes, build-vs-buy research
  first per standing law.
- lifecycle match arms (user 2026-07-28 design thread): every atom is a
  delta envelope (sign + = next, - = finalize, scope close = complete);
  bare atom = sugar for the + arm (the Result ?-unbox analogy); `match`
  reserved for subscription-time arms + envelope enums. Unruled, needs
  spelling + fixture work.
- `input/distinctUntil(shallowEquals|deepEquals)` on rels — mostly already physics
  here (R7 boundary diffing = distinctUntilChanged at every rel edge; set/keyed
  identical writes are zero-delta); the real residue is WHICH columns count as
  identity (= the Key/Q8 ruling) and digest-vs-value for structured blob columns
  (the content_hash pattern).

### v6 rulings RESOLVED 2026-07-27 late PM (three grunts; rulings.pl is the record)
- **salt_minting = content_addressed** ("one hunt"): shared in-flight effects, IVM
  support refcounting for free, freshness = explicit extra salt column. Consequence:
  **stale_fill_policy = not_applicable** — under content salts a fill is a cache
  update, never stale; no orphan rel, no fill tick-item, no per-instance identity.
- **effect_abort = best_effort_cancel_on_support_zero** ("rope arrow" + the
  invariant: "no arrow stop exist, is lie" — cancellation is cost optimization,
  never semantics; warn-paint at the abort site + debug line per attempt). Lowering
  owed: AbortSignal through HostDef.run + cancel map + pending-row delete (ARCH task
  effect_abort).
- **subscription_kernel = minimal_with_coverage_check_and_ghost_view**: zero stored
  rels, zero new phases; obligations = scope-coverage static check (ARCH task
  scope_cover_check, answers the zombie-scope break) + ghost forest diagnostic view
  (ARCH task ghost_forest_view). Shared DRed-depth hazard (recursive rels in scope
  cones = f(depth) statements vs n1_statement_budget) filed separately, owner
  unassigned.

### v6 REORIENTATION (user-set 2026-07-27 night): TSV2, prolog compiles TO TypeScript
NEW PRIMARY EFFORT (plans/2026-07-27-tsv2-compile-target-header.md): prolog owns
the whole compiler front (parse/AST/typecheck/lowering); it EMITS literal
TypeScript program files with the real SQLite statements and real rxjs chains
visible in the generated file. TypeScript keeps only (a) a hand-written static
runtime reusing the NAMED v6 symbols (SqlRunner, spine.ts fact plane, IVM
machinery, HostRunner lift, P0 tracing channels — class-34 law, import-gate
checked) and (b) the generated gen/*.ts programs. No AST/parser/lowering in TS
on this path. v6/dl stays untouched and running as the sibling; langium/
ast_bridge/lower are dead weight for tsv2 only. Grading = the item-9 tick-log
JSONL diffed byte-for-byte against the prolog oracle (the 109-fixture corpus is
the compiler test suite). Phases: A hand-carved target exemplar (2 scopes
fixtures) -> B prolog emitter matches it byte-identically -> C fixture sweep ->
D .dl DCG surface + hosts (ghcacher rides D). The stopping-point program list
below still defines DONE; programs land against the tsv2 target as it matures.

### v6 STOPPING POINT (user-set 2026-07-27 late PM): express the real programs
The milestone that ends this arc: the real programs written in the v6 surface and
graded, zero new constructs unless a program PROVES a gap (extraction-lab discipline):
1. **ghcacher** (poll -> fetch -> cache -> change_log carry; mode-lattice prog facts
   are the draft; content-addressed salts now ruled, so SWR spelling is open).
2. **diags for LSP** (diag rels -> diag_v5 view; the lsp-v5-bridge receipt is live).
3. **git pre-commit --changed** (biome-style recent-change-lines gating, generic and
   directable from dl).
4. **sprefa-extract run**: scan/scanwork, repo/rev extraction, lazy finding, lazy
   heads.
5. **auto-synced repo list**: HEAD the repo list itself (repo rows the system keeps
   synced; v5 repo-rev-scanning receipts research in flight).
6. **v5 bench parity target**: the v5 multirepo crawl benchmarks (grafana-class
   corpora) are the perf yardstick the v6 expressions must eventually meet.
7. **rtkq examples through sprefa-extract**: the redux-toolkit-query example corpus
   as an extraction+analysis target program.
8. **file watcher scaling, cross-platform preferably** (i:file-watching skill is the
   reference; watcher is a BIND per spine_residency, never kernel).
9. **standardized tick-log format**: the per-tick delta log serialized in ONE stable
   format (the marble record) so later runners (rust, python, ts) are graded by
   diffing logs against the oracle's log, never by embedding in the language. This
   is the json-rx cross-target agreement record made concrete.
Directive riding the milestone (rulings.pl spine_residency): the git/fs spine is
HOSTED IN THE LANGUAGE (stdlib rels + binds + salts over generic effect machinery),
never kernel; where the native concepts fail to host it intuitively, that is a
language finding, not a reason to special-case the spine.

### v6 rulings 2026-07-28 AM (rulings.pl is the record)
- **typed columns** (tsv2): int decls -> INTEGER storage; compounds stay
  inline-flat (punt); nested/reference storage model (struct-as-rel +
  surrogate id, the intern-dictionary pattern) BANKED as a future header.
- **unmarked edge triggers confirmed**: any-body-atom occurrence model (not
  whole-world), only() = opt-in restriction. C2 LANDED 2026-07-28 (merge
  c32dba53, coordinator re-ran the sweep to identical counts): typed columns
  (C2a, int/text inference per literal witness) + unmarked triggers (C2b) =
  scoreboard 9 identical/8 wrong/92 unsupported -> 27/0 wrong-diff/79
  (3 residual = pre-existing run_error/no_oracle fixtures). Named refusals
  banked: edge_trigger_is_derived (needs a tickLoop carry seam, ownership
  crossed so refused), edge_head_column_type_mismatch (2), edge_head_
  conflict_risk (1). Next unsupported buckets by size: edge_marked_with_
  extra_goal 21, comparison-in-level-body 14, aggregate_head 9, pre 8.
  C2 also fixed: both prolog test harnesses' hardcoded dead-worktree path,
  sweep.pl stale-output off-by-one, boot t=0 level closure over Initial.
- **clock_residency = world_fed_bind_not_construct** ("clock bind yes"):
  cadence enters as ordinary bind rows; SWR = rules over latest state joined
  with the clock rel; F2 gap dissolves at zero construct cost.
  LANDED 2026-07-28 (merge 378a39cf, sonnet agent): `1_binds.ts`
  BindDef/BindRunner = input twin of HostDef; activation by EDB rel-name
  match; commits$ merged beside effects$ in runProgram$ so program-swap
  switchMap kills bind timers; clock bind reads clock_period rows -> one
  interval per distinct period, bucket = floor(epoch_secs/period)
  (restart-stable); sprefa:bind tracing channel. Coordinator re-verified:
  typecheck clean, dl 90/90 (+4), ratchet 1, endurance PASS. Known limits
  (1_binds.ts header): clock_period config read once at subscribe (mid-run
  row needs reload to spin a new interval); no input-side dedupe cache
  (cadence has no witness — real asymmetry vs effect_cache). Agent side
  finding: bare fact `clock_period(2).` compiles to an IDB rule over a
  minted __lit_0 seed, not EDB. Follow-up open: ghcacher.dl gains real SWR
  via a clock_period row.
- **MERGED TO MAIN**: main fast-forwarded to the cleanup tip (aed4c155 ->
  9f8b6edc, 92 commits).

### v6.2.0 TAGGED (2026-07-29, local tag on e931191e, push = user)
P3 LANDED (merge e931191e, codex sol no-commit flow, coordinator
verified: sweep 34/31/0, conformance 115, roundtrip, plunit 28/28,
tsv2 6/6, gate, tsgo, store js 74/74): retraction = emitted
support-count SQL, guard per rule graph (plain count acyclic /
recursive-CTE reseed where cycles reachable), P2's three fallbacks
removed with fixture receipts, SIGKILL-mid-CTE recovery PASS.
COMPETITION ENTERED (PERF-REPORT.md standings, same input hashes):
tsv2-from-node DAG 60k 24.5ms / 240k 98.7ms / 960k 429.2ms BEATS rust
sqlite-count (31.3/105.1/443.0) at 23 stmts; CYC 960k 2756.5ms
CORRECT via reseed where rust bare count is wrong. Common memory
columns live in the shared CSV (host_peak_mb; sqlite_hw_mb =
N/A-with-reason, @libsql exposes no memory_highwater API; db_mb).
Honest cracks: support seeding not delta-proportional inside the
seed statement; multi-head recursive strata + multi-self-read
clauses = named unsupported; rust kernel_roots test skipped under
git law (writes .git/worktrees). FULL GATE ON THE TAGGED COMMIT:
conformance 115/0, sweep 34/31/0, roundtrip ALL PASS, plunit 28/28,
tsv2 6/6 + gate, dl 96/96, store 74/74, ratchet 1/1, endurance END
GOAL HOLDS. NEXT (user-agreed order): edge-off-derived carry seam,
then match block sugar.
HOSTS+EXTRACTION LAB LANDED (merge d7ac6926, lab-death 39f0733d, last
copy 2199456d, verdict plans/2026-07-29-hosts-extraction-verdict.md;
coordinator re-ran 41 PASS x2 + conformance 115/0 + roundtrip): term
inventory for the follow-up wiring arc = sh_decl/4 (EXPLICIT
input/output split, template edit never silently flips mode),
probe/4 (salt = plain column, identity vs witness digests split),
bind_decl/2 (decl authorizes, name links; zero-decl rel-name
activation REFUSED as the magic-rel hazard; `bind interval(...)`
selected, `clock` refused by the rx-name law), query/1 (whole rel
atom retained), ts_query/1 (12/12 tree-sitter query features mapped,
compiles to exact query text, unknown forms = named refusal),
sg_pattern/3 (own family; ts_query coercion refused,
slot_sg_metavariable_semantics). EXTRACTION FORK VERDICT: sg/ast/
tree-sitter/span take the HOST shape (EDB arrivals content-addressed
on (file_digest, query_digest), 1 invocation across N rules, feeds
edge rules as ordinary deltas); decode/json_each stay the
term-extract precedent. Ambiguities: A12 + A1 RESOLVED (push bind is
distinct from demand host; glob = host demand column), A4 + A14 stay
open with named slots. 5 fixture/5 candidates distilled for wiring.
DL6 DOOR LANDED (merge 9d096dd6, codex luna no-commit flow,
coordinator re-ran EVERYTHING in the worktree AND
conformance+roundtrip+text-door on merged main): compile_dl6/2 text
entry in compile.pl + compile_dl6.sh runner; text_door_receipt.sh =
34/34 byte-identical term-door vs text-door over the sweep's
compiled set; hand-written door-handwritten.dl6 (colon types + enum
+ latest + log) tick log byte-identical to the oracle; dl_view +
v6/dl/fixtures renamed to .dl6 (v5 .dl untouched); vscode grammar
dl6.tmLanguage.json GENERATED from registry.pl (emit_dl6_grammar/0),
dl6 language id contributed, extension compiles. Grades: conformance
115/0, roundtrip ALL PASS, sweep 34/31/0, plunit 28/28, tsv2 6/6 +
gate. Receipt scratch out/text-door gitignored, stripped from the
landing.
SQLITE UDF GRAFT LAB LANDED (merge 4bcf9aba, lab-death 9de6cddb, last
copy 9084850d, verdict plans/2026-07-29-sqlite-udf-graft-verdict.md;
coordinator re-ran PASS x5 stdout twice + conformance 115/0 +
roundtrip, which the agent had correctly fenced out of): v5 has 14
DISTINCT UDF names across 16 call sites (header's 16 was call sites);
usage: replace_re 38+21, regexp-as-=~ 78+13, split 34, lines 3+7 are
the hot ones, sym_intern used ZERO times in examples/.dl. DRIVER
REALITY: @libsql/client 0.17.4 has NO UDF registration API at all
(all four candidate method names undefined) -- the current TS seam
cannot register UDFs, period; better-sqlite3 .function() and sql.js
create_function both proven working, rust sidecar registration
proven, node-sqlite3 fails to load on node 24 arm64 (named slot).
GRAFT SHAPES per class: core SQL fuses where semantics match
(lower/upper/trim; parity 15-16/16, the misses are Unicode edge
rows); regex needs a function or sidecar (JS-compatible subset 15/15,
rust inline (?s) unparseable in JS); TS deopt proven delta-only (no
full-table scan receipt); emit-time for constants. Q4 ASSERTION SET
(P1 8 items / P2 5 / P3 8) handed to the running lift agent for
summary-time reconciliation. Q5: sprf_sym can feed content_id only
with type salt + canonicalization; dense intern mates stay
storage-only, never semantic identity (consistent with the types-r2
surrogate-mate ruling). Named slots: LIBSQL_UDF_API unresolved (the
eventual driver decision), INTERN_SIDE_EFFECT staging,
NODE_SQLITE3_ABI.
EXPRESSION+AGGREGATE LIFT LANDED (opus worktree agent, 6 commits,
merged; coordinator re-ran conformance 120/0, sweep BOTH modes 60
compiled/57 identical/0 wrong, plunit 40/40, tsv2 12/12, gate,
roundtrip on merged main): comparisons/arith/:= binds/concat/head
arithmetic fused into emitted SQL (WHERE + SELECT expressions);
aggregates count/sum = per-group accumulators, min/max = insert
delta-compare + GROUP-SCOPED delete recompute (EXPLAIN receipts:
SEARCH via PK, 1 of 5000 groups touched; sabotage receipts in test
headers); compiled 34 -> 60, identical 31 -> 57, conformance 115 ->
120 (+5 oracle-verified fixtures incl 2 pinning cross-type join).
Fail-first receipts red->green: TEXT-collapse (plunit
expression_miscompile_guards) + @libsql REAL bind corruption
(bootBind.test.ts -- BOTH harness boot loops bound params raw, the
one path skipping int->bigint). NEW final-state grading leg in the
sweep (closes the empty-schedule vacuity + makes keep(count)
non-lowering VISIBLE: final_wrong 3, all pre-existing). Q4
reconciliation caught a THIRD miscompile class: cross-type join
under affinity conversion ('1' vs 1 join = 1 row where oracle
derives none) -- now join_column_type_mismatch refusal. json
agg heads STAY refused: ordering reproducible in SQL but the
tick-log encoder renders prolog cons text ([|](4,...)), not json --
encoding gap, not order gap. Named cracks: edge bodies still refuse
comparisons/binds (no guard seam in the arrival-projection arm);
braces/list VALUES now refuse (were silently storing "null" /
{}(...) -- the phase-C "identical (vacuous)" braces row was a
miscompile); reconcile-frontier asymmetry commented in place.
POST-MERGE DEFECT, coordinator-found: text_door_receipt red on main
-- 20 lifted fixtures type via SCHEDULE literal witnesses, printed
.dl6 views carry no decls so the text door refuses
arith_operand_not_int; PLUS the receipt hardcodes =:= 34. Proven
fix: typed decls in the view text compile clean through the door
(hand receipt in chat). Fix = synthesize inferred colon-typed decls
into dl_view emission + dynamic receipt gate. Dispatched in the
3-lane blast.
3-LANE BLAST LANDED same sitting (all base 622dda3e, all re-verified
by coordinator in-worktree AND on merged main -- final merged
battery: conformance 120/0, roundtrip ALL PASS, TEXT_DOOR 62/62/0
exit 0, sweep both modes 62 compiled/59 identical/0 wrong (final 58
identical/3 pre-existing), plunit 47/47, tsv2 12/12 + gate, dl 96/96
(+1 soak skip under plain npm test), store 74/74, leak-soak 5
receipts green):
(1) TEXT-DOOR FIX (sonnet, merge after 394aacbe): print_dl
synthesizes colon-typed decls for WITNESSED undeclared EDB refs
(witness-less refs excluded -- freezing analyze's open(none) into
text broke 9 timeless_rail fixtures, found empirically); receipt
gate now dynamic (all term-door-compiled must pass text door),
two-stage grading replaces the silent skip reclassification;
sabotage receipt in header. Crack: witness check is ref-granular
not column-granular (noted in print_dl.pl header).
(2) EDGE-CARRY SEAM (codex sol, merge 78919aea):
edge_trigger_is_derived refusal REMOVED; derived edge triggers read
P1 frontier tables via the incremental dispatch; door program
byte-identical oracle-vs-tsv2 ticks 1/2/3 -- THE ENUM STATE MACHINE
IDIOM NOW COMPILES. Promoted edge_chain_hops_tick_per_stage +
demand_view_fires_its_consumer_once. Carry counts flat 100 vs 10k
rows, indexed frontier SEARCHes. Named crack: derived-trigger
programs use the incremental path even under
SPREFA_TSV2_EMITTER_MODE=naive (snapshot path has no delta stream).
MATCH BLOCK NOW UNBLOCKED per the user-agreed order.
(3) ENDURANCE GATE + NO-LEAK SOAK (codex luna, merge 9c1ffb4b):
green-all now includes endurance + leak-soak; leak-soak.sh = 20
swap/commit/SSE cycles then 5 receipts (handles/resources flat by
type via getActiveResourcesInfo, RSS bounded post-warmup +25%,
stmts-per-tick 10==10 via DL_PERF_LOG, SSE inner subs 0, bind
Timeout 1==1 across swaps); sabotage receipt in header; the three
law-debt soft spots (commits$/reportsSubject, server.close/readBody
wrappers, HostRunner boot replay) all CLEAR under soak.
ALSO FIXED on main pre-blast-merge (394aacbe): dl6-door rename had
broken 5 fixture paths in v6/dl tests/golden (dl suite was 89/96 on
main since the door merge, caught by the soak lane's baseline; the
door-arc merged-tree re-verify hadn't included the dl suite).
RULED (rulings.pl tail): json_ticklog_encoding = canonical_json_text
(json agg heads become emittable; oracle encoder change + one-time
regrade = the follow-up arc); udf_residency =
libsql_fuse_and_delta_deopt. STILL AWAITING USER (re-asked with
explanations): keyed-on-level-head refuse-vs-define, keep(count)
lowering choice.
RECURRING FOOTGUN, unowned: sweep regen DELETES non-fixture modules
from gen_emitted/ (door-handwritten.ts dropped THREE times this
sitting, restored each time); fix is sweep.ts leaving unknown files
alone or door-handwritten becoming a fixture.
MATCH + RULINGS LANE LANDED (merge 05f8ad29, codex sol no-commit
flow, coordinator re-ran in worktree AND merged main: conformance
126/0, sweep both modes 66 compiled/63 identical/0 wrong (final
63/2, both pre-existing runtime-error fixtures), plunit 54/54, tsv2
14/14, TEXT_DOOR 66/66/0, roundtrip ALL PASS, dl 96/96): match/2
block sugar via ONE shared expand_match_program
(v6/prolog/0_match_expand.pl, oracle + compiler both consult, the
enum-expansion precedent), arms expand to ordinary rules, enum
coverage checked (match_nonexhaustive refusal), sugar vs
hand-desugared tick logs byte-identical (sha b93e3028);
keyed_level_head refusal LIVE in oracle + compiler (fail-first
inert-accumulation receipt recorded; keyed edge head still
replaces); keep(count) LOWERED: one set-based DELETE...RETURNING
into the negative-delta path pre-P3, 12 statements flat at 3 vs 100
arrivals, retention_count_prunes_oldest final_wrong -> IDENTICAL.
+6 fixtures (4 compiled + 2 named refusals). Parser/printer/
registry/SYNTAX/tmLanguage all carry match blocks.
RULED same sitting (rulings.pl tail): keyed_level_head =
named_refusal; retention_count_lowering = retracting_rule_over_log
(both executed by this lane). ALSO RULED: json_ticklog_encoding =
canonical_json_text (regrade arc pending, unowned); udf_residency =
libsql_fuse_and_delta_deopt. USER DIRECTIVES 2026-07-29 late: CLI
("the bop") gates the 6.2.0 push -- registry.pl grows a cli command
table, emitter targets COMMANDER (required) on the TS side + clap
derive later, verbs serve/run/check/load/q, run+check boot the
server in-process (server-calls-itself, no daemon concept); spine
stays hosted per spine_residency with worktree as the UNMARKED
default source (no "WORK" atom -- pinned rev is the marked case);
kwargs partial application queued (task: body atoms may omit
columns = fresh wildcard, heads stay total; parse_dl
fill_free_slots :590 is the current exact-fill gate).

### Hands-on findings 2026-07-29 (coordinator wrote+ran a cold program; scratch fixture, receipts in chat)
- **keyed() on a level-rule head is SILENTLY INERT** (F8/retention-inert
  defect class): keyed(current/2,[1]) + `current(Id,Tag) <- door_tag(Id,Tag)`
  accumulated BOTH rows for key 1, no replace, no refusal (oracle
  engine.pl: decl_key consulted only in apply_edge_writes). Needs either
  a named refusal or defined replace semantics -- user call which.
- **edge_trigger_is_derived now blocks the flagship enum idiom**: the
  natural state machine `current(Id,Tag) <+ door_tag(Id,Tag)` runs in the
  oracle but REFUSES in tsv2 (banked C2 refusal), and the enum tag view is
  derived BY CONSTRUCTION, so enums + edge rules never compose in compiled
  programs. The banked refusal just got a lot more central; the fix is the
  tickLoop carry seam the C2 agent declined for ownership reasons.

### v6 still awaiting user word (small, none blocking the absorption arc)
- **Q8 residual**: confirm left-of-arrow = demand key on effect rels, `Key()` never
  appears there (the shipped TS reading; extraction lab's preference).
- **filesize rail + lazy-rel-tier + dom-match rewrite** (v5 side, unchanged).
- Tabling question CLOSED (plans/2026-07-27-tabling-verdict.md): SHIFTS SEMANTICS,
  hand-rolled fixpoint stays (the not_stratified guard IS semantics).
- **extraction ambiguities** A12 (from-world = nullary `->`?), A1 (glob residency),
  A4 (fence escape), A14 (comment_span bind). plans/2026-07-27-extraction-spellings.md.
- **Key(Type) vs `->`**: labs split three ways; present both files' arguments, no fiat.
  plans/2026-07-27-lab-consolidation.md bottom.
- Queued smaller: operators.pl models forkJoin as a level rule (correct only while
  inputs are unscoped — refixture when the sub forest absorbs); `scope_done`
  read-by-name violates the magic-rel ban (needs a decl); repeat's arrival-tick salt
  collides on two same-tick resubscribes; `until(F)` formula presentation in CLI output.

### Worktree dispatch law (user-set 2026-07-28, applies to every agent at every level)
- Every worktree agent's FIRST action: `git merge --ff-only <sha>` where the
  coordinator's prompt states the exact current main sha. If that fails, or the
  worktree is missing expected trees, STOP AND REPORT. Working around a blocked
  command through another mechanism (archive/tar, --no-verify, manual copying)
  is a defect, never a fix — a permission denial ends the approach, full stop.
- The coordinator verifies the agent's base sha in its first report and refuses
  work built on any other base (cherry-pick at most, never a history merge).

### Lab protocol (user-set 2026-07-27, applies to every agent at every level)
- **Planner seeds the header first.** Every lab starts from a planner-written contract
  file: the predicates/checks the lab must implement, the questions it must grade, and
  named slots for ambiguities it may discover. No lab starts from a blank file.
- **Implementation agents run in worktrees** (Agent `isolation: "worktree"`), never in
  the main tree. Main-tree file ownership belongs to the coordinator only.
- **Labs die on landing.** In the same arc that a lab lands: durable output distills to
  its permanent home (conformance/fixtures, rulings.pl, plans/, ARCH.pl), the lab files
  are deleted, and the plan doc records the commit hash holding the last copy
  (`git show <hash>:<path>` recovers it). Git history is the archive.
- `v6/prolog/labs/` was deleted 2026-07-27 (last full copy at 2fff3f61) and stays
  deleted; a lab file surviving its landing commit is a defect, not a follow-up.

### Style notes for this repo
- **Language vocabulary law** (user-set 2026-07-28): construct names and design
  discussion use ONLY rxjs, prolog, or SQL words. No invented terminology.
  Consequence under review: `only()` -> `latest()` (withLatestFrom), explicit
  `combine`/`zip`, `departed` -> rx-word candidate.
  ENFORCED 2026-07-29 on "support" (user: datalog-paper jargon, out):
  the concept is refCount, row-granular (count of derivations keeping a
  row alive; zero = teardown; cycles never reach zero = the Rc leak).
  Prose uses refCount NOW; identifier rename queued (rust store
  supportEdges/supportPlan/retractThroughSupport, lowerSql supportPlan,
  P3 emitter names) -- luna-shaped mechanical sweep, dispatch on word or
  fold into the next emitter-touching arc.
- **Every .dl snippet shown to the user carries its intended pure-rxjs
  lowering** (user-set 2026-07-28: "if u cant then we are not right"). A
  construct whose rx lowering cannot be written is a design defect.
- **Formerly-quadratic paths get COUNT tests** (user-set 2026-07-28): any
  path that was ever O(n^2) gets a test asserting the operation count/plan
  (statement counts, EXPLAIN QUERY PLAN SEARCH-not-SCAN), never end-state
  equality alone. Additive tests only; do not ravage working code for
  purity. Tracing/logging state in a single JSON file is acceptable.
- dl variable names are descriptive, never single-letter: `path`/`line`/`callee_name`, not `p`/`l`/`q`. Applies to every snippet in skills, examples, book, tests, and agent prompts; rename opportunistically when touching old files.
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- **Every new class declares its interface in the package's header `types.ts`** (user-set 2026-07-25): a
  class that ships without a contract in the header is an incomplete change, not a follow-up. The
  header declares each name exactly once, no `export type Foo = SomeFoo` aliases. v6 headers are
  `v6/sprefa-store/js/src/engine/types.ts`, `v6/sprefa-store/js/src/lower/types.ts`,
  `v6/dl/src/0_types.ts`. Currently uncovered: `tasks.ts` `Namespaced`/`Independent`/`Evidence`,
  `engine.ts` `AscendingIdQueue`. `Error` subclasses are exempt.
- **Important functions are interface-bound, never bare `export function`** (user-set 2026-07-25):
  TypeScript cannot conformance-check a standalone function against anything. A free
  `export function foo()` can drift from its documented signature and the compiler stays silent.
  So any function that matters gets bound to a header interface one of two ways:
  - namespace object, the default: `interface ISqlRunner { ... }` in the header,
    `export const SqlRunner: ISqlRunner = { ... }` in the module.
  - a class `implements` the interface, when there is real per-instance state or arg-object envy.
  The annotation is what buys the check. `satisfies` also checks and additionally keeps the
  literal's narrow inferred type; use it only when a caller needs that narrower type.
  Small leaf helpers that would be a `.map` callback or a plain method call in another language stay
  bare functions. This is the same exemption as the rxjs law.
- **Interfaces carry the `I` prefix** (user-set 2026-07-25): `IStore`, `IGraphNs`, `IDlRuntime`.
  The prefix is what lets the interface and its implementing object hold the same root word
  without an alias. `lower/types.ts` (`RelTable`, `Graph`, `Stratum`, `IDatalog`) is inconsistent
  and is the rename target, not the other way round.
- **Exactly ONE manual `.subscribe()` in the whole app, ever** (user-set 2026-07-25): React does
  not ask you to call `ReactDOM.render` three times. One terminal subscription at the bottom of
  `main.ts`; everything above it is cold and composed with `merge`/`concatMap`. A second
  `.subscribe()` anywhere is a design failure, not a style preference, because it means that
  branch of the graph is started imperatively and its lifetime is tracked by hand.
  Corollary: no `Subscription` field held on a class, and no `Subject` used as a request/response
  bridge (a method that pushes into one Subject and awaits a matching id on another is RPC wearing
  a stream costume, and it forces every caller back into `await`).
  Ratchet: TARGET REACHED 2026-07-27 (baseline = 1, never rises): the one site is
  `dl/src/main.ts` subscribing `serveDl(...)`. Remaining law debt, not ratchet debt:
  the `commits$`/`reportsSubject` Subject pair (3_runtime.ts) vs the no-Subject-bridge
  corollary, and the `server.close`/`readBody` Promise wrappers above the seam.
- **A type name must say what the thing is on first reading** (user-set 2026-07-25): no
  library-flavoured or abbreviation names that carry no content. `Rx` is the rejected example.
  If one interface needs a vague name it is usually two interfaces glued together; split it and
  both names get obvious.
- **Async becomes rxjs; sync stays sync** (user-set 2026-07-25, CORRECTING the earlier
  "make the whole code rxjs" instruction, which the user withdrew: "i should not have said
  make it all rxjs, just make the async into rxjs"):
  - `Promise`/`async`/`await` are banned above the single driver seam. That seam is
    `SqliteDb.execute`, wrapped exactly once in `SqlRunner` (`engine/sqlRunner.ts`).
  - Loops, branching, and list building over in-memory data stay **plain array code** and
    **return arrays**. `map`/`filter`/`flatMap`/`reduce`, not `from -> concatMap -> toArray`.
    A function that computes a `string[]` returns `string[]`.
  - The dividing line that works in practice (see `lower/lowerSql.ts`): SQL *building* is
    sync and returns statements; only *running* statements is an Observable. `runAll` is the
    single place a `string[]` becomes execution.
  - Symptom that the line was crossed: an Observable pipeline that ends by throwing its
    values away (`count()`, `toArray()` then ignore, `ignoreElements()`). That is sync work
    wearing an Observable. It also hides real values, which cost 8 redundant
    `SELECT count(*)` scans per conformance run before `rowsAffected` was let through.
  - `Observable<never>` is not used here. An effect emits one `void` when done and callers
    chain with `concatMap`; `concat` would union the effect's type into the value type.
  TRAP: `await someObservable` returns the observable without subscribing and TypeScript accepts it
  silently. Use `firstValueFrom`, or better, do not leave an `await` to convert.
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
