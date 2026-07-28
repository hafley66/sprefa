# v6 pinned decisions — STOP re-deriving these

> **How this pin got made:** we mined our own session logs + raw Claude Code
> transcripts to recover decisions we kept re-deriving. See
> `v6/findings/SELF-RESEARCH.md` for the method (rg the `.jsonl` transcripts, 8
> haiku passes over chat_log) and `v6/findings/SESSION-DIGEST.md` for the lineage.


If you are about to re-open "counting vs DRed", "how do cycles work", "weight
semantics", "acyclic fast-path", or "are these graph algos the same thing":
DON'T. It is decided (6+ times). Pointers below.

## THE UNIFICATION (the thesis, settled — do not re-argue it)

Salsa reactivity (dep dirty-propagation), SCC decomposition, reachability /
blast-radius, and dd/feldera Z-set incremental maintenance are **the same graph
counting algorithm**. They sound eerily alike because they ARE one thing:

    ONE semi-naive cascade:  frontier → one hop → prune → fixpoint

The ONLY thing that varies is the prune predicate (`v6/ARCHITECTURE.md:73,108,144`):
  - A · control (salsa)     → prune by **digest** (early-cutoff)
  - B · facts (dd/feldera)  → prune by **weight ≠ 0** (Z-set counting)
  - C · reach (SCC/blast)   → prune by **reached**

Unifying them in SQLite is HIGHLY VIABLE and is the plan. The remaining work is
NOT choosing an algorithm — it is **empirically deriving each path's Big-O and
driving it down** (measured, not asserted). This is what the perf harness is for
(`examples/perf_report.rs`, `profile_dred.rs`, `explain_plans.rs`). The point of
all of it: kill v5's resident 36GB-swap model by keeping state on disk (RSS
bounded by page cache, Rust heap ~0), while matching the resident engines on
correctness and driving the counting Big-O down toward them on speed.

## The retraction / recursion model (DECIDED)

Source of truth: **`v6/plans/2026-07-19-v6-table-design.md:344-368`**.

- Retraction is NOT a separate code path. A delta is `(row, ±weight)`; apply =
  one upsert that adds weights and deletes at zero. No "retract" verb.
- Weight = support count (3 rules → weight 3; kill one → weight 2, survives).
  Arithmetic answers it for the acyclic case. "Feldera is this, and nothing
  more exotic than this."
- Explicitly NOT adopted: salsa (resident memo), differential-dataflow (resident
  arrangements — the "you have enough RAM" assumption that fails at 500 repos and
  is the v5 36GB swap nightmare we are killing).
- Weight is INTEGER support-count; `weight>0` = alive. Boolean-bit REJECTED
  (`chat_log/20260721.1...md:58`).

**Retraction is CLOSED 2026-07-23 (owner ruling; the earlier "nested fixpoint"
pin described unimplemented code — verified by grep the same day).** Production
retract is three golden-gated pieces, in both `v6/sprefa-store/src/engine.rs`
and `v6/sprefa-store/js/src/engine/engine.ts`:
- **counting Z-set retract** (acyclic), the weight upsert above.
- **`retract_scc`** (cycles), a two-pass over-delete/rederive
  (`engine.rs:296` delegates to `retract_scc_two_pass` at `:304`). The prior
  pin's "nested fixpoint" has 0 hits in `engine.rs`; the `scc_scope`/
  `scc_frontier`/`scc_next`/`scc_live` TEMP tables are created but never
  referenced by any other line; that framing was phantom.
- **`retract_dred_cte`**, the set-at-once DRed variant, also golden-gated and
  shipped, not merely an oracle.

All three are cross-checked against the survivors oracle and against each
other; none is a stray comparison harness kept out of production.

Supporting: `v6/ARCHITECTURE.md` (one semi-naive cascade, prune =
digest·A / weight·B / reached·C). DRed derivation was in the deleted
`v6/labs/labkit/WHY-DRED.md` — `git log --follow` it if needed.

## The TS engine + rxjs lowering (DECIDED 2026-07-23)

- The reactive engine is **TS on ACTUAL rxjs** (Observable / Subject /
  BehaviorSubject + a BufferPolicy knob). NOT a Rust rx re-implementation. json-rx
  is EXTRACTED from an rxjs graph (round-trip proof), not a lowering target.
- The Rust crate keeps its job: the SQLite cascade (Reach / Cascade / Reconcile /
  GraphStore) + extraction (WASM/CLI). It is ported 1:1 to TS at
  `v6/sprefa-store/js/` so the rxjs layer calls the same knobs + reads the same
  SQLite. Golden-gated 11/11, peak RSS 141 MiB. dd/salsa stay Rust-side oracles,
  NOT ported to TS.
- The **fixpoint stays in SQLite** (the cascade). rxjs owns the control plane
  (demand, dirty, wake, compose) — the part v5's global tick did badly. Re-doing
  Z-set IVM in TS is the resident-RAM trap the unification killed.
- **BOOKMARK (owner, 2026-07-23):** groupBy / aggregation / latest-by-gen lower
  INTO SQL (`GROUP BY` + `LIMIT`) at the `dirty` boundary, never into TS arrays.
  Plan: `v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md`.

## TS SQLite bindings are FROZEN (owner ruling 2026-07-23 PM)

No further binding-level work in `v6/sprefa-store/js/` — no lib swaps on engine
paths, no leak-chasing, no upstream filing. The TS side is the prototype lab; the
SQLite data plane returns to Rust at the json-rx generation point (rest-epic plan
E9). The measured libsql native RSS creep (~0.3-0.4 MiB per 100 execute calls,
better-sqlite3 flat on the identical workload; receipt in the E1 stress gun,
`js/src/labs/stress.ts` header — deleted 2026-07-28 per the labs-die-on-landing
protocol, `git log --follow` it if needed) is ACCEPTED lab noise; stress gates
are set above it. Agents: do not "fix" this.

## Rel retention forms: rel(0) / rel(1) / rel (owner ruling 2026-07-23 PM)

Retention is the decl's one capacity knob; no `chan` keyword, no separate
state/event kinds:

| form | keeps | late subscriber | rx twin | role |
|---|---|---|---|---|
| `rel(0)` | nothing (rows live only inside their arrival tick) | misses it | Subject | event |
| `rel(1)` | newest row | readback | BehaviorSubject | state |
| `rel` | all rows | full history | table | history |

`rel(N)` for N>1 (ReplaySubject window) is NOT ruled in; window-join semantics
need a law first. Joins against `rel(0)` are same-tick-only (Bloom scratch).
`latest(x)` (time agg, see chat_log/20260723.6.learning-lloyd-topor-free-variables.md)
is the query-side view of what `rel(1)` retains. CSP/chan discussion that led
here: Bloom table/scratch/channel, Dedalus async = next-with-unknown-delay,
CHR keep/consume — consuming reads stay OUT of the fixpoint core.

## Tick column shape: (b) current + delta log (owner ruling 2026-07-24)

Rel tables stay flat (current rows, reads unchanged). One append-only delta log
`delta(rel, row_digest, tick, weight)` stamped once per commit batch at the
single write site (with_txn / ingest commit); the store-owned monotone tick
counter persists in store_meta. `latest`/`prev`/as-of read the log; reconcile's
changed_at becomes a real column (kills the recycled-changed_at hazard).
`rel(0)/rel(1)/rel` retention = purge policy on log rows. The datomic-style
all-append-only shape (a) is NOT taken; revisit only if git-axis time-travel
becomes primary. Two time axes stay separate: tick = engine commit counter,
rev = git coordinate found by walking the spine (span -> file -> rev -> repo),
= ANSI SQL:2011 SYSTEM_TIME vs application-time.

## Surface rulings, fork session 2026-07-24 (forked-for-sql-pipeline-syntax)

- Time builtins take rxjs names VERBATIM (interval/timer/delay/debounceTime/
  throttleTime/auditTime). clock/every die as names. Law: no synonyms when an
  rx target name exists. Store spellings underneath (no subscription-local state).
- Effect sigils POSTFIX: `rel?(args)` idempotent effect (digest-cached),
  `rel!(args)` mutation (fire-once, never replayed). Auto Result<T,E> = error
  lands as columns (QueryState shape). Postfix because it marks the TIMECUT:
  the atom where the body splits across ticks (host stage = yield point).
  `!` prefix stays negation; `!x!(a)` legal; sigil mechanics = v7 problem if ever.
- Idents are slash-liberal (lisp-style): `gh/pull_request` is one ident; types
  addressable as URL paths. `/` binds into idents unless spaced (division needs
  spaces); regex literals are value-position only.
- Comma stays UNORDERED (join order = planner SIPS). Pipe `|>` syntax shelved;
  its semantics survive: a host/temporal atom in a body is the tick-boundary
  cut; compiler splits the rule there (pre-effect request rel = saved frame =
  live vars; post-effect rule wakes on response rows). Minted intermediates are
  rel(0) scratch; cross-tick stage rels are durable (the effect cache).
- Types are tuples now; struct/named types later. "rel" keyword stays (not
  "table").
- Time sources are effects: `interval!(300, bucket)` — rx name + postfix `!`
  (joining one cuts across ticks by definition).
- diag = plain `rel`, read by the LSP plugin. No state/event kind. Diags are
  the FIRST RETRACTION INSTANCE: file fix -> re-extract -> old facts retract ->
  diag rows die through the delta plane with zero diag-specific code (the DRed
  golden test). `--check` = a reader (severity=error -> exit 2), one line, open.
- Type system = JSON5 shapes: nested object/array/primitive. NO generics.
  Named shapes later; types addressable as slash-path idents. Whole system =
  JSON shape + Key/Min/Max wrappers + column base types.
- gen rename: v4 archaeology (archive 2026-07-01, last work 2026-05-18/19)
  shows v4 already had the right vocabulary — mounted_query.rs, dirty_source.rs,
  memo.rs, re-render loop, owner identity. Candidates: render/emit/mount.
  Codegen = render-to-file, memoized by content hash, retracted like any
  derived row (= write-host-rel + re-ingest + idempotence). Name unruled.

## How to re-find any past decision (the commands that work)

```bash
# raw Claude Code transcripts (the real conversation, ranked by hit count):
rg -i -c 'PATTERN' ~/.claude/projects/-Users-chrishafley-projects-sprefa/*.jsonl \
  | sort -t: -k2 -rn | head
# then pull phrases without dumping the 500MB file:
rg -i -o '"[^"]*PATTERN[^"]*"' ~/.claude/projects/-Users-chrishafley-projects-sprefa/<uuid>.jsonl | head

# session summaries + design decisions:
grep -rniE 'PATTERN' chat_log/*.md plans/ v6/plans/ v6/*.md

# decision docs live in plans/ and v6/plans/ (dated YYYY-MM-DD-topic.md)
```
