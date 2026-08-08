# Lane I-E: head shape, re-measured after THE FLIP

Base `1a675434a363e24d8b3f7a69dcafcbfa747cc93d`, branch `lane/i-e-rowid-heads`,
worktree `sprefa-lanes/ie`. Mandate: `plans/2026-08-08-interning-contract.md` §7.

## TOC
- [1. The answer, in one table](#1-the-answer-in-one-table)
- [2. The constant, re-derived with its direction](#2-the-constant-re-derived-with-its-direction)
- [3. Why the constant was ambiguous: two 16%s pointing opposite ways](#3-why-the-constant-was-ambiguous-two-16s-pointing-opposite-ways)
- [4. The flip decision and its license](#4-the-flip-decision-and-its-license)
- [5. Two factual errors found in §7's table](#5-two-factual-errors-found-in-7s-table)
- [6. §12 bench: dict vs direct, first post-flip measurement](#6-12-bench-dict-vs-direct-first-post-flip-measurement)
- [7. §5.3: the decode-hoist receipt](#7-53-the-decode-hoist-receipt)
- [8. G1: intern share of tick time](#8-g1-intern-share-of-tick-time)
- [9. Gate receipts](#9-gate-receipts)
- [10. Residual risks](#10-residual-risks)
  - [10.1 Commit `9a5889a2` carries two unrelated defects, both found here](#101-commit-9a5889a2-carries-two-unrelated-defects-both-found-here)
- [11. Reproducing](#11-reproducing)

---

## 1. The answer, in one table

| question | answer |
|---|---|
| which side of the ambiguous constant owns the slowdown | **rowid+UNIQUE** is the slower one, by 5.4-7.6% on the flagship head |
| which side owns the memory win | **WITHOUT ROWID**, by 2.4x on database bytes |
| does interning change the call | no; WITHOUT ROWID wins with TEXT keys AND with INTEGER keys |
| do recursive fixpoint heads flip | **NO.** Null result, and the null is the deliverable |
| what the rowid actually buys | the rowid-range DELTA (17-53%), never the storage on its own |
| dict vs direct, whole fixpoint | 1.69-1.94x faster, 9.0-9.4x smaller on disk |
| G1 intern share | 0.24% at 10k input edges, 5.96% at 1M; the SQL door itself is 0.27% |

Zero lines of `lower.pl` changed. The lane lands the measurement, the skill
correction, and the contract note.

## 2. The constant, re-derived with its direction

Shape: `flow_reach` exactly as the emitter writes it today for a level-headed
rel, mirrored from
`v6/prolog/compile/out/flagship_flow_reach_over_batched_resolved_edges.ts:153`
— four key columns plus `__refcount`, every column INTEGER after THE FLIP,
values interned through a real `__str` built from the `.tin` corpus.

The ping/pong wavefront is held byte-identical across both arms, so **storage
is the only variable**.

| case | WITHOUT ROWID | rowid+UNIQUE | rowid penalty | db MB WOR | db MB rowid | bytes ratio | RSS WOR | RSS rowid |
|---|---|---|---|---|---|---|---|---|
| grid_10000 | **1,296 ms** | 1,387 ms | **+7.0%** | **15.0** | 35.5 | 2.37x | 78 MB | 98 MB |
| chain_10000 | **13,083 ms** | 14,076 ms | **+7.6%** | **148.4** | 360.2 | 2.43x | 241 MB | 468 MB |
| layered_10000 | **14,598 ms** | 15,385 ms | **+5.4%** | **143.8** | 353.8 | 2.46x | 224 MB | 417 MB |

Best of 3 on grid, best of 2 on chain and layered. Every arm agrees on
`derived` and on the pair fold, so the three arms computed the same relation:
grid 1,069,200 / `63595584321739:82741762593251`, chain 9,996,213 /
`1186664984663702:1365126280247375`, layered 9,951,396 /
`810808610034685:1173402340196180`. The derived counts match the corpus's
banked values.

**Direction, stated so it cannot be lost again: rowid+UNIQUE is the slower and
fatter shape. WITHOUT ROWID is the faster and leaner one.** The mechanism is
the one already in the skill: an index IS a copy of its key, so rowid+UNIQUE
stores every key twice while WITHOUT ROWID stores it once.

The call survives the flip. Measured in TEXT space on grid for the control:
`wor_text` 2,511 ms / 141.4 MB against `rowid_unique_text` 2,678 ms / 273.7 MB,
a +6.6% rowid penalty and the same 1.9x bytes ratio. Interning moved the
absolute numbers by 1.9x and left the ORDERING of the two shapes untouched.

### What the rowid does buy

The delta mechanism, isolated on identical rowid+UNIQUE storage:

| case | ping/pong | rowid range | gain | statements |
|---|---|---|---|---|
| grid_10000 | 1,387 ms | 1,183 ms | 1.17x | 265 -> 89 |
| chain_10000 | 14,076 ms | 9,226 ms | 1.53x | 7,744 -> 2,582 |
| layered_10000 | 15,385 ms | 11,904 ms | 1.29x | 577 -> 193 |

17-53%, and it is the only reason to want a rowid head. It is a delta
restructure, not a storage change.

## 3. Why the constant was ambiguous: two 16%s pointing opposite ways

`sqlite_raw/REPORT.md`'s own variant race carries both comparisons, and both
land on 1.164:

| pair | ratio | what differs |
|---|---|---|
| `loop_range_rowid` 9,798 vs `loop_notexists_wor` 11,406 | 1.164 | the DELTA (rowid range vs ping/pong) |
| `loop_notexists_wor` 11,406 vs `loop_notexists_rowid` 13,275 | 1.164 | the STORAGE (WITHOUT ROWID vs rowid+UNIQUE) |

Finding 6 of that report quoted the first ratio and attributed it to the
second. The skill inherited the sentence with the subject dropped, and a reader
had no way to recover the sign. The coincidence that the two ratios are equal
to three digits is what made the error survive review.

Corrected in place at `v6/labs/exec_shootout/sqlite_raw/REPORT.md` finding 6
and at `.claude/skills/sqlite-costs/SKILL.md`.

## 4. The flip decision and its license

**Decision: do not flip. Recursive fixpoint heads stay WITHOUT ROWID.**

The contract authorised a flip only if rowid+UNIQUE won for recursive heads
carrying `fixpointIr`. It loses on both axes at every scale measured, with
TEXT keys and with INTEGER keys. There is no reading of the data that licenses
the change.

The rowid-range delta does win, and it is out of this lane's scope by the
contract's own text: it replaces the ping/pong walk, whose PK-order scan is
ordering law property 2 (offload contract §4.2), so it moves `_sequence` for
every program and owes a `_sequence` receipt this lane was not asked to
produce. §7 already fences wave/ping/pong/cone as WITHOUT ROWID for exactly
that reason.

Because nothing flipped, §5.2 row 8's `_sequence` prediction has nothing to
check against: no fill order moved, no tick-log bytes moved, and the corpus is
byte-identical (§9 gate a).

## 5. Two factual errors found in §7's table

Row 2 of the §7 table asserted `recursive level head -> rowid + UNIQUE, the arm
lower.pl:930 already writes`. Both halves are wrong.

| claim | reality |
|---|---|
| "the arm `lower.pl:930` already writes" rowid+UNIQUE for recursive heads | `lower.pl:930` is `head_select_list/7`, unrelated to DDL. `rel_ddl/6` (`lower.pl:1201-1245`) emits `"__id" INTEGER PRIMARY KEY ... UNIQUE (...)` under one condition only, `declared_type_name(Types, Name)`. Level-headedness adds `__refcount` and nothing else. The arm does not exist, so the flip would have been new code, not a re-use |
| rowid+UNIQUE is the right shape for those heads | measured backwards, §2 |

Confirmed against the emitted artifact: `flow_reach` is level-headed (it
carries `__refcount`) and is emitted `WITHOUT ROWID`. Also note `rel_ddl` is
arity 6, not the `rel_ddl/5` the contract names in §7 and §10.

## 6. §12 bench: dict vs direct, first post-flip measurement

Same head, same wavefront, same corpus; the only difference is whether the four
key columns hold interned ids or raw path/name strings. This is the first time
the arc's headline claim has been measured on a whole fixpoint rather than on
the insert alone.

| case | direct (TEXT) | dict (INTEGER) | speedup | db MB TEXT | db MB INT | bytes | RSS TEXT | RSS INT |
|---|---|---|---|---|---|---|---|---|
| grid_10000 | 2,511 ms | **1,296 ms** | **1.94x** | 141.4 | **15.0** | **9.4x** | 215 MB | 78 MB |
| chain_10000 | 25,426 ms | **13,083 ms** | **1.94x** | 1,342.4 | **148.4** | **9.0x** | 1,428 MB | 241 MB |
| layered_10000 | 24,694 ms | **14,598 ms** | **1.69x** | 1,310.9 | **143.8** | **9.1x** | 1,445 MB | 224 MB |

The 1.69-1.94x band sits inside the 1.68-1.99x the arc promised from the insert
microbench, so the win does NOT decay when the insert is embedded in a join
loop with a `NOT EXISTS` gate against the same table. Layered is the low corner
at 1.69x.

**The 9x bytes number is new and was not part of the arc's claim.** It is the
larger practical effect: chain drops from 1.34 GB to 148 MB and peak RSS from
1,428 MB to 241 MB, a 5.9x memory reduction on the flagship shape.

`headInsertMs` in the raw JSONL is NOT a clean G2 receipt and is deliberately
not quoted as one: it drives inserts one row at a time through the N-API
boundary, which costs ~600 ns per row and dilutes the key-shape difference to
1.47-1.51x. The fixpoint column above is the SQL-driven number and is the one
that answers G2.

## 7. §5.3: the decode-hoist receipt

Two questions, both closed, neither requiring a code change.

**The literal subquery is hoisted.** Statement taken verbatim from the emitted
corpus (`INSERT OR IGNORE INTO "body_tag" ... (SELECT s."__id" FROM "__str" s
WHERE s."content" = 'page') FROM "body_page" b0`):

```
SCAN b0
SCALAR SUBQUERY 1
SEARCH s USING COVERING INDEX sqlite_autoindex___str_1 (content=?)
```

`SCALAR SUBQUERY`, not `CORRELATED SCALAR SUBQUERY`, and the opcode trace
carries exactly one `Once` (addr=6, `Once p1=0 p2=15`) gating the subquery's
`OpenRead`/`SeekGE`. It is computed once per statement.

Timing confirms it at bench scale, 1,000,000 rows, best of 3:

| spelling | ms |
|---|---|
| emitted scalar subquery | 108 |
| `?` bind parameter (§5.3's named fallback) | 108 |
| id spliced into the SQL text | 103 |

**The fallback is not needed.** A 5 ms spread over 1M rows is 4.6%, and the
bind arm matches the subquery arm exactly. §5.3 can drop its contingency.

**The read-side decode is real but absent from the hot path.** Decoding to
compare costs 2.9x an id comparison:

| spelling | ms | plan |
|---|---|---|
| `(SELECT content ...) = 'page'` | 76 | `CORRELATED SCALAR SUBQUERY`, per row |
| `tag = (SELECT __id ...)` | 26 | `SCALAR SUBQUERY`, once |

That 2.9x is exactly what §5.3 rule one buys by sending identity demand to the
id. And the flagship never pays it: **the flagship module's fixpoint SQL
contains zero `__str` decodes.** All ten `__str` mentions in that module are
the two ingest-door statements plus the `__txt_*` boundary views. The
"decode 3x per frontier row" the lane was asked to price does not occur on the
flagship, because its rules carry no text operand; the decode lives only at the
render boundary, where it is one correlated probe per output row.

## 8. G1: intern share of tick time

Gate: intern share of load+fixpoint+materialize <= 4.5%.

| case | read | intern | load | fixpoint | materialize | intern share |
|---|---|---|---|---|---|---|
| grid_10000 | 3 ms | 3.06 ms | 7 ms | 1,284 ms | 0.11 ms | **0.24%** |
| chain_1000000 | 346 ms | 646.81 ms | 1,430 ms | 8,425 ms | 1.01 ms | **5.96%** |

The 1M case is over the 4.5% gate. The decomposition says why, and it is not
the dictionary:

| piece of the 646.81 ms | ms | share of intern |
|---|---|---|
| JS `Set` build over 4M string slots | 385.02 | 60% |
| rewriting 999,989 edges to id tuples through the `Map` | 225.80 | 35% |
| the two emitted SQL statements (`internSql` + `lookupSql`) | 29.53 | 4.6% |
| `JSON.stringify` of the 21,103 distinct strings | 6.46 | 1.0% |

**The SQL door costs 29.53 ms, or 0.27% of the 10,849 ms total, and issues
exactly 2 statements** — which is what §6.4's COUNT rail actually pins and what
G1's failure clause ("the door is doing per-row work") is written to catch. By
that reading the gate passes with 16x of headroom.

The 5.96% is JS-side `Set`/`Map` work over 4,000,000 string slots in the
harness's own ingest path. `REPORT-INTERN.md`'s 4.33% was the same pass written
in rust with an `FxHashMap`. The delta between 4.33% and 5.96% is the language
the dedup is written in, not a dictionary cost.

**Recommend the coordinator restate G1 against the SQL door** (statements and
their ms) rather than against a wall clock that includes the host language's
hash-set build, or else re-baseline the number for the TypeScript runtime.
Flagged rather than silently passed.

## 9. Gate receipts

| gate | expected | measured | verdict |
|---|---|---|---|
| c. `swipl -g go -t halt ARCH.pl` | 7 PASS | 7 PASS | green |
| b. plunit | 474 | 474/474 run, 0 failed, 0 ERROR, exit 0 | green |
| d. conformance `go.pl` | 308 PASS / 0 fail | **310 PASS / 0 FAIL** | green, count noted below |
| a. `scripts/sweep.sh` | RUN wrong=0 / FINAL wrong=0 / crash=0 | `RUN total=213 identical=212 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0` / `FINAL total=213 final_identical=212 final_wrong=0 no_oracle_final=1` / `MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0` | green |
| e. `pnpm test` | 188/187/1skip | `tests 188 / pass 187 / fail 0 / skipped 1`, 7.31 s | green |

The one `rejection` and one `no_oracle_final` are the same fixture,
`log_retraction_rejected`, whose oracle throws on the schedule by design.

The sweep also regenerated two stale emitted modules; see §10.1 defect B.

Conformance reports 310 PASS, two more than the stated 308 baseline, with zero
failures. This lane changed no Prolog, so 310 is the base's own count; the
coordinator should reconcile the baseline number rather than read this as
movement.

## 10. Residual risks

| # | risk | detail |
|---|---|---|
| 1 | **a committed symlink destroys the main tree's `node_modules` on every checkout** | see §10.1. Blocking; it broke this lane mid-run and will break the next one |
| 2 | the delta restructure is now the only live head-shape lead | 17-53% is the largest single number this lane measured and nobody owns it. It needs a `_sequence` receipt against ordering law property 2 before it can be scoped |
| 3 | G1's denominator is ambiguous between the SQL door and the host-language pass | §8. Two defensible numbers, 0.27% and 5.96%, and the gate does not say which it means |
| 4 | the bench measures a hand-written mirror of the emitted head, not the emitted module | the DDL and the wavefront are copied from the flagship artifact and the derived counts match the banked corpus, but a divergence between mirror and emitter would not be caught here |
| 5 | chain and layered are best-of-2, not best-of-3 | the storage gap (5.4-7.6%) is larger than the run-to-run spread observed on grid, but a third run was not taken at those scales |
| 6 | `.tin` inputs are regenerated, not committed | `gen_text` is seeded and deterministic and the int twins matched the banked `.in` corpus by construction; the inputs themselves live outside the repo |

### 10.1 Commit `9a5889a2` carries two unrelated defects, both found here

`9a5889a2` ("enum: a bare enum name works as a column type") committed two
things it did not mean to, and this lane tripped over both.

**Defect A: two absolute-path symlinks are tracked in the repo.**

```
120000 v6/dl/node_modules   -> /Users/chrishafley/projects/sprefa/v6/dl/node_modules
120000 v6/tsv2/node_modules -> /Users/chrishafley/projects/sprefa/v6/tsv2/node_modules
```

Each points at its own checkout path in the MAIN tree. Checked out anywhere
else the link resolves into the main tree, which is the worktree convention and
works. Checked out **in the main tree it points at itself**, so `git checkout`
or a branch switch there replaces the real `node_modules` with a self-
referential symlink and the dependencies are gone (`Too many levels of symbolic
links`). That is what happened at 14:39 during this lane's run: both paths in
`/Users/chrishafley/projects/sprefa/v6/` are now ELOOP, and every worktree
that links there has no dependencies.

The main tree was not touched by this lane. Repair is `pnpm install` in
`v6/tsv2` and `v6/dl` of the main tree AFTER the tracked links are removed;
removing them (`git rm --cached`, plus a `.gitignore` entry) is a coordinator
call because it changes every worktree's layout, so it is reported rather than
done here.

**Defect B: the same commit left two emitted modules stale.**
`out/enum_name_is_a_column_type.ts` and
`out/enum_nullary_variant_boots_and_tags.ts` were never regenerated after THE
FLIP and still carried pre-intern DDL (`"tag" TEXT NOT NULL`, no `__str`, no
`TEXT_INTERN_PLAN`). `scripts/sweep.sh` regenerates before it diffs, so every
sweep passed while the committed corpus disagreed with the compiler. **G11
("every `text` column in every emitted module is INTEGER") was red at base on
those two files and is green now**; the regeneration is included in this
lane's commit as a mechanical fix with no `lower.pl` change behind it.

After regeneration the corpus holds 8 non-`__str` TEXT columns across 11
modules (`concat_program_queue`, the four `json_*capture*`, `pairwise_*`,
`keyed_replace_departs_the_old_row`, `departed_fires_next_tick_on_retraction`,
`finalize_over_log_fires_on_retention_prune`,
`ordered_json_group_array_nested_json`). Those are §3.1's automatic `direct`
fallbacks (contract §5.2 row 13, computed text in head position), not
findings — but **G11 as written in §12.2 calls "a single TEXT column outside
`json` a finding", so the gate needs the §3.1 fallback carve-out spelled out
or it fails green code.** Flagged for the gate's owner.

## 11. Reproducing

The bench needs a working `v6/tsv2/node_modules`, so §10.1 defect A has to be
repaired first; `bench_one.mjs` loads `libsql` out of that pnpm store.

```
cd v6/labs/exec_shootout/intern_bench && cargo build --release --bins
./target/release/gen_text --family grid --scale 10000 --out $W/grid_10000.tin --also-int $W/grid_10000.in
cd ../head_shape
node race.mjs --work $W --runs 3 --cases grid_10000 \
  --arms wor,rowid_unique,rowid_range,wor_text,rowid_unique_text
node bench_one.mjs --input $W/chain_1000000.tin --arm wor    # G1
node hoist_probe.mjs --rows 1000000                          # §5.3
```

`head_shape/` is lab code and dies on landing under the lab protocol; the
numbers above and the two corrected files are its durable output.
