# Lane: Mercury enters exec_shootout + the retraction tick bench

## Base
Coordinator states the base at spawn (`--base-sha` from origin/main); your
FIRST action is `git merge --ff-only <that sha>`. Failure = STOP AND REPORT.
Repo: `~/projects/sprefa`. Worktree: `.boop-worktrees/feature/exec-shootout-mercury`.

## Why this exists
`v6/labs/exec_shootout/` compares interp / rxgraph / mono (compiler-emitted
Rust) on semi-naive reachability. Chris wants a Mercury entrant to see where
a compiled logic language lands on the SAME contract, and a first retraction
number, because no engine in the lab is incremental (Rust dd would be, and
pricing that gap is the point of the tick bench). Receipts, not opinions.

Prior receipts on this machine (`~/projects/sprefa-v6/docs/labs/mercurypl/`):
naive map+set_tree234 TC ran 1.1M derived tuples/s; mono runs 39-68M rows/s
with u32 + FxHashMap + unrolled semi-naive. Your Mercury engine must play in
mono's data-structure class: dense arrays, unique modes, no 234-trees on the
hot path.

## Read first
- `v6/labs/exec_shootout/CONTRACT.md` (the whole file; the CLI + JSONL + fnv
  checksum + semi-naive requirement are non-negotiable)
- `v6/labs/exec_shootout/STANDINGS.md`
- `v6/labs/exec_shootout/harness/` to learn how engines are registered/run
- `v6/labs/exec_shootout/mono/src/main.rs` for the algorithm shape to match
- `~/projects/claude-research/skills/mercury-lang/SKILL.md` (working notes:
  toolchain, error->fix table, uniqueness modes)

## Deliverable 1: `v6/labs/exec_shootout/mercury-semi-naive/`
A Mercury program satisfying the CLI + IO contract exactly:
- `--input <path>`; parse `p <nodes> <edges>` header then `u v` pairs.
  `io.read_file_as_string` + a hand scanner; do not tokenize with regex.
- Same two-rule reachability, semi-naive REQUIRED, single thread.
- Data structures: CSR-style adjacency (`array(int)` offsets + targets),
  per-source seen sets as dense bitsets (`array(uint64)` or `bitmap` module)
  with destructive update through unique modes. If unique-mode array update
  fights you, record the friction verbatim and fall back to the fastest
  compiling representation; the friction IS a finding.
- Events on stdout exactly per contract: `loaded`, `fixpoint` (derived count
  + ms), `done` (fnv1a64 XOR checksum lowercase hex, peak_rss_kb via
  `getrusage` through `pragma foreign_proc("C", ...)`).
- Build: `mmc -O5 --make`; a small `build.sh` + `README.md` (one page:
  representation choices, and the mercury-vs-mono structural differences).
- Register it with the harness the same way the other engines are
  registered; touch ONLY the registration point (name it in your report).
- Validate checksum equality against mono on chain 10k and grid 10k before
  timing anything. A checksum mismatch = STOP, fix, only then measure.
- Measure best-of-3 per contract at whatever scales complete in sane time
  (10k and 100k minimum; 1M if under ~2 min per run). Append a clearly
  marked `mercury-semi-naive` section/rows to `STANDINGS.md`; never edit
  existing cells.

## Deliverable 2: the tick bench (retraction, first number)
New dir `v6/labs/exec_shootout/tick_bench/`:
- `TICKS.md`: the bench definition. K=100 ticks; each tick removes one
  existing edge and adds one new edge (deterministic seed, document it),
  then requires the full correct `reachable` count. Metric: median and p95
  per-tick wall ms, plus total.
- A driver script (bash or a tiny rust/mercury program) that runs the bench
  against ANY contract engine by re-invoking its binary per tick with a
  rewritten input file: this measures the from-scratch re-solve floor, which
  is what every current engine actually costs per tick.
- Run it on `mono` and on your `mercury-semi-naive` at chain 10k and grid
  10k, 1 warmup + 3 measured passes, record all numbers in `TICKS.md`.
- Close `TICKS.md` with the gap statement: per-tick from-scratch cost vs the
  delta-proportional cost an incremental engine (dd-class) would pay,
  expressed with your measured numbers (no dd implementation; the user
  decision is dd arrives through an emitter, and this bench prices WHY).

## Ownership
You own ONLY `v6/labs/exec_shootout/mercury-semi-naive/**`,
`v6/labs/exec_shootout/tick_bench/**`, additive rows in `STANDINGS.md`, and
the single harness registration point. Forbidden: every other path in
sprefa, `interp/ rxgraph/ mono/ dl6/ sqlite_*` engine sources,
`~/projects/sprefa-v6`, `~/projects/hafley-rs`. Mercury build junk
(`Mercury/` dirs, `.mh/.mih`, binaries) never gets committed.

## Validation before you finish
```bash
./mercury-semi-naive/build.sh
# checksum parity, chain 10k + grid 10k, vs mono
grep -c mercury v6/labs/exec_shootout/STANDINGS.md   # > 0
ls v6/labs/exec_shootout/tick_bench/TICKS.md
git status --short | grep -v '^??' | awk '{print $2}'  # only files you own
```
Commit on your branch with receipts in the message. PR posting is the
coordinator's step, not yours.

## Style laws (non-negotiable)
- No em dashes. Banned words, prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" banned: write "not built yet".
- Comments state only what the code cannot show. Tables over prose in docs.
- Every number in STANDINGS/TICKS carries machine + date + run count.
- 10-second law: any single run over 10s gets investigated before being
  normalized into the bench loop (1M scale excepted if measured and noted).
