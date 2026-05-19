# Ph7 — Constant-RSS Proof + Perf Regression Guard + DD Removal

Status: PLAN (rewritten to use the EXISTING harness). 2026-05-17. Parent:
`plans/2026-05-17-retraction-fixpoint-dred-no-dd.md` (Ph7 row). Prereq:
Ph0–Ph6 landed on `feat/retraction-ph6` (rebased onto current main).

## Correction to the prior draft

The first draft invented a `synth_corpus` + criterion harness. That was
wrong: a real harness already exists and is what every prior perf number
came from. Ph7 reuses it and adds only the assertions on top. Do NOT add
criterion/divan or a synthetic corpus.

Existing harness (root `justfile`):
- `just fixture-linux` — provisions `v3/tests/smoke/.fixtures/linux`
  (full kernel tree, 36,785 `.c`, ~2 GB, gitignored, submodule-like).
- `just v4-bench-linux[-warm|-quick|-store|-store-warm]` →
  `v4-bench --root <linux> --workers N --trials T --batch B
   --pattern <p> --lang c --mode bare [--sprf-store] [--warm-page-cache]`.
  Telemetry already prints `wall`, `rss_peak_MB`, `matches`,
  `parse_ms`/`match_ms`/`read_ms`, fs stage timings.
- `just v4-bench-linux-sprf[-telemetry|-join|-antijoin]` →
  `sprefa-run v4/bench/<linux*.sprf> --root <linux> --no-show-rows
   --telemetry --batch B --fact-db <db>`. This is the path that actually
  exercises rules → memo / `SUPPORT.mult` / `reconcile` / stratified
  negation. The bare `v4-bench` path does NOT (memoize=off, no rule).
- `just v3-bench-linux` → the v3 `ast_grep_v3_bench` parity reference.

## Recorded baseline (this session, machine = this Mac, fixture above)

`v4-bench`, 63,482 files, 1.34 GB C, `printk($$$)`, 8 workers:

| run | MAIN `345bd67f` | PH6 `bb1a2f97` |
|---|---|---|
| bare, median wall (3 trials) | 4.151 s | 4.342 s |
| bare rss_peak | 213–231 MB | 210–243 MB |
| sprf-store, wall (1 trial) | 4.341 s | 4.349 s |
| sprf-store rss_peak | 250 MB | 255 MB |
| matches (correctness) | 16627 | 16627 |

`.sprf` rule-workload numbers (`linux.sprf`, `linux-antijoin.sprf`,
MAIN vs PH6) — captured separately this session; fold the measured
values into `perf/baseline.toml` in 7a (do not hand-transcribe a guess).

Reading: the always-on scaffolding (Ph0 double cursor hash, Ph4b
key_terms, the ever-present driver `MemoSeam` probe with `None`) costs
≈ nothing on the parity path (bare +4.6% is a cold-cache trial-1
outlier; trials 2–3 == MAIN; sprf-store +0.2% wall, +2% RSS). The
memo/retraction-active cost is the `.sprf` number, which is the one
Ph7 gates.

## What Ph7 proves (unchanged)

1. RAM does not track corpus size.
2. One file change recomputes only its dependency slice.
3. Ph0–Ph6 did not regress the bulk path beyond a fixed cap.
Then: remove the dead `differential-dataflow`/`timely` deps so "no DD"
is a fact, not an intention.

## Layer 1 — signatures (thin additions only; reuse v4-bench/sprefa-run)

```rust
// perf/baseline.toml  (NEW, checked in) — captured on the gate machine
//   [v4_bench.bare]   median_wall_s = 4.151   rss_peak_mb = 231
//   [v4_bench.store]  wall_s = 4.341          rss_peak_mb = 250
//   [sprf.linux]      wall_s = <measured>     rss_peak_mb = <measured>
//   [sprf.antijoin]   wall_s = <measured>     rss_peak_mb = <measured>

// v4/tests/perf_gate.rs (NEW) — drives the EXISTING release binaries,
// parses their telemetry stdout, asserts vs baseline.toml. No criterion.
fn run_v4_bench(root: &Path, store: bool) -> BenchTelemetry;     // spawn release v4-bench, parse
fn run_sprf(script: &str, root: &Path) -> SprfTelemetry;          // spawn release sprefa-run --telemetry, parse
struct BenchTelemetry { wall_s: f64, rss_peak_mb: u64, matches: u64 }
struct SprfTelemetry  { wall_s: f64, rss_peak_mb: u64, rows: u64,
                        owners_rendered: u64, owners_replayed: u64 }

// v4-bench gains ONE new mode for assertion #2 (the only new product code):
//   --poke <relpath>   : scan, snapshot telemetry, mutate that one file,
//                        drive the dirty-source loop, print before/after
//                        {owners_rendered, owners_replayed, rss_peak_mb}.
//   This reuses Ph6's dirty-source→re-render-owner loop; it is a CLI
//   entrypoint to it, not new runtime logic.

// linux subset = the corpus-scale knob (NO synthetic corpus):
//   small  = <linux>/kernel        (~1.5k .c)
//   mid    = <linux>/drivers/net   (~3k .c)
//   full   = <linux>               (~37k .c)   ~24x small
fn rss_slope(small: BenchTelemetry, full: BenchTelemetry, n_ratio: f64) -> f64;
```

## Layer 2 — the three pinned assertions

```rust
// 1. CONSTANT RSS. scale corpus 24x via linux subdirs, not file count math.
//   s = run_v4_bench(linux/"kernel", store=true)
//   f = run_v4_bench(linux,          store=true)
//   files_ratio = f.files / s.files            // ~24x
//   assert (f.rss_peak_mb as f64 / s.rss_peak_mb as f64) < RSS_SLOPE_CAP
//   // 24x files, < 2x RAM ⇒ sublinear ⇒ disk holds the corpus, not RAM
//   assert f.matches > s.matches               // work scaled, not skipped

// 2. ONE POKE = ONE SLICE.  needs memo/dirty loop ⇒ use --poke (rule .sprf).
//   b = run via `v4-bench --poke kernel/sched/core.c --sprf-store --memoize`
//   assert b.after.owners_rendered <= SLICE_CAP        // small, NOT f(N)
//   assert b.after.owners_replayed >= b.before.rows - SLICE_CAP
//   assert (b.after.rss_peak_mb - b.before.rss_peak_mb) < RSS_POKE_DELTA_MB

// 3. REGRESSION GUARD vs baseline.toml (env-overridable on a new machine).
//   bare = run_v4_bench(linux, store=false)
//   assert bare.wall_s        <= BASE.bare.wall_s        * (1+REG_CAP)
//   assert bare.rss_peak_mb   <= BASE.bare.rss_peak_mb    * (1+RSS_CAP)
//   sp = run_sprf("linux.sprf", linux)                    // memo/support live
//   assert sp.wall_s          <= BASE.sprf_linux.wall_s   * (1+REG_CAP)
//   assert sp.rss_peak_mb     <= BASE.sprf_linux.rss_peak_mb*(1+RSS_CAP)
//   aj = run_sprf("linux-antijoin.sprf", linux)           // Ph6 stratified neg
//   assert aj.wall_s          <= BASE.sprf_antijoin.wall_s* (1+REG_CAP)
```

## Layer 3 — lifetimes

| Thing | Lifetime | Note |
|---|---|---|
| linux fixture | persistent, gitignored | `just fixture-linux` ensures; never re-clone per worktree |
| `perf/baseline.toml` | checked in, env-overridable | captured once per gate machine |
| fact-db (`.sprf` runs) | one run, `/private/tmp` | `rm -f` before and after each run |
| telemetry structs | stack-local in `perf_gate.rs` | parsed from one stdout, asserted, dropped |
| `--poke` mutation | one run | mutate a copy under tmp, never the real fixture file |

## Layer 4 — order / uniqueness

- Build RELEASE binaries first (`cargo build --release --bin v4-bench`
  and `--bin sprefa-run`); debug numbers are meaningless. The session
  baseline above is release.
- Sample RSS once per run, from the existing `rss_peak_MB` telemetry
  line (v4-bench/sprefa-run already emit it; macOS byte/KB normalization
  already handled — do not re-derive).
- Disk guard MUST use `df -k` then divide; `df -g` is not portable
  (it errored every check this session and silently never guarded).
  Abort, never auto-delete caches.
- `--poke` mutates a tmp copy of one `.c`, drives the Ph6 loop, and the
  uniqueness condition is: `owners_rendered` after a 1-file poke is
  independent of total corpus size (that IS the constant-RSS claim made
  operational).
- The `.sprf` workload is the only one that exercises Ph3–Ph6; the bare
  path guards the always-on scaffolding only. Both gates are required.

## Phasing (each a commit on a Ph7 branch off `feat/retraction-ph6`)

| Ph | Deliverable | Gate |
|---|---|---|
| 7a | `perf/baseline.toml` populated from measured release numbers (this session's table + the `.sprf` numbers); `perf_gate.rs` spawn+parse harness, no asserts, prints captured vs baseline | builds, prints |
| 7b | Assertion #1 (constant RSS via linux subdirs) | #1 green |
| 7c | `v4-bench --poke` (CLI entry to Ph6 dirty loop) + assertion #2 | #2 green |
| 7d | Assertion #3 (regression gate, bare + linux.sprf + antijoin) vs baseline.toml | #3 green |
| 7e | delete `differential-dataflow`+`timely` from `v4/Cargo.toml`, delete `v4/src/_attic/dd.rs`; full suite + perf_gate green; dep tree shrinks | green, DD gone |

7e is the formal close: no-DD is proven only when the deps are gone and
the constant-RSS gate still passes.

## Thresholds (tune in 7a from measured reality, never relax to pass)

| Const | Start | Meaning |
|---|---|---|
| `RSS_SLOPE_CAP` | 2.0 | 24x corpus ⇒ < 2x RAM |
| `SLICE_CAP` | 8 | owners re-rendered per 1-file poke, corpus-independent |
| `RSS_POKE_DELTA_MB` | 32 | RAM bump from one poke |
| `REG_CAP` | 0.10 | wall ≤ 10% over baseline |
| `RSS_CAP` | 0.15 | rss_peak ≤ 15% over baseline |

A blown cap is the single most important output of this phase. Report
it; do not move the number.

## Risks / notes

- The bare parity path is ~flat (proven this session). The risk lives
  entirely in the `.sprf` memo/support/reconcile path — gate that hard.
- `--poke` is the only new runtime-adjacent code; it must call the same
  Ph6 dirty-source loop, not a parallel reimplementation, or it proves
  nothing.
- linux subdir file counts drift across kernel versions; compute the
  ratio at runtime from `fs_rows`, do not hardcode 24x.
- Keep `just v3-bench-linux` parity reference runnable; if Ph0–6 ever
  regresses bare below v3 parity that is a release blocker, not a Ph7
  cap.
