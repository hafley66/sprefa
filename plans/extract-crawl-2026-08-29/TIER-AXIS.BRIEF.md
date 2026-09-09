# Lane `feat/extract-tier-axis` (opus): one measure interface keyed on `lang.family.tier.oracle`

First action: `git merge --ff-only 41333391a95c601dfcc8f7372bcfaa6ea72c5d76`.
Failure or missing tree = STOP, beep the coordinator, do not work around it.

## Goal, one sentence

`tier` becomes a first-class column of the bench harness so every accuracy
number this repo quotes is produced by ONE code path
(`score_case(Case{lang, family, tier, oracle})`) and lands in a committed
ratchet row, replacing today's arrangement where tier is inferred from `lang`
and the ts checker numbers exist only in a markdown table.

## Why (zero-context reader)

`plans/extract-bench-2026-08-29/COMMON.md:52-76` is the user's measure-signature
law: every accuracy number carries `⟨lang.family.tier⟩ vs ⟨oracle⟩ on ⟨corpus,
n files⟩`, and `COMMON.md:75` fixes `Tier ∈ {syntax, checker, scip}`.

The harness cannot express that key. Receipts:

| defect | receipt |
|---|---|
| tier is derived from lang, not chosen | `v6/sprefa-extract/tests/bench/mod.rs:733` `let checker = corpus.lang == "rust";` |
| the ts checker tier is unreachable from the harness | `tests/bench/mod.rs:748` `ts_checker: None` (hardwired) |
| RATCHET.tsv has no tier column | `tests/bench/mod.rs:833` header is 8 columns, `lang family oracle recall precision wall_ms rss_mb sha` |
| so its rust rows are checker-tier and its ts5 rows are syntax-tier, unlabelled | `plans/extract-bench-2026-08-29/RATCHET.tsv` rows 5-8 vs 9-11; `v6/justfile:83-88` runs rust with `--features cli,rust-checker` and ts5/go without |
| the ts checker number is therefore hand-typed prose | `plans/extract-crawl-2026-08-29/ts.REPORT.md:637` `97.33 / 70.36`, produced by the 138-line python twin `plans/extract-crawl-2026-08-29/ts5.checker.measure.py`, which states at its own line 7 "Nothing here writes RATCHET.tsv" |
| cost columns duplicate | `RATCHET.tsv` rust rows all carry `13465 / 3756`; wall and rss are per-extraction, stored per-oracle |
| projections are selected by matching an oracle FILE NAME string | `tests/bench/mod.rs:291-306` (`GoProjection::per_oracle`), `:441-455` (`RustProjection::per_oracle`) |

The user's ask, verbatim: "i want a common interface to how we are testing the
$lang.$fam.$tier".

## Design (implement exactly this; do not redesign)

### 1. Types, in `tests/bench/mod.rs`

```rust
/// The three tiers of COMMON.md:75. Nothing else is a tier.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Tier { Syntax, Checker, Scip }

impl Tier {
    /// The RATCHET.tsv spelling and the `--exact` test-name suffix.
    pub fn as_str(self) -> &'static str;   // "syntax" | "checker" | "scip"
    pub fn parse(text: &str) -> Tier;      // panics on anything else
}

/// Both sides of one comparison, after projection.
pub struct Sides { pub ours: BTreeSet<String>, pub oracle: BTreeSet<String> }

/// Which shape rewrite a case scores under. NAMED on the case row; never
/// selected by matching an oracle file name (that is the magic-rel shape the
/// repo bans; see .claude/skills/sprefa-v5-no-magic-rels for the reasoning).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Projection { Raw, GoMethod, GoImpl, RustCorpus }

/// One measured cell = one RATCHET.tsv accuracy row.
pub struct Case {
    pub lang: &'static str,     // ts5 | go | rust
    pub family: &'static str,   // call | type | module
    pub tier: Tier,
    pub oracle: &'static str,   // file name under BENCH_DIR
    pub projection: Projection,
}

/// One extraction. Every Case sharing (lang, tier) scores off ONE of these:
/// the extraction is the expensive half, scoring is free.
pub struct Run {
    pub files: usize,
    pub files_rel: BTreeSet<String>,
    pub wall_ms: u128,      // median of 3
    pub rss_mb: u64,        // getrusage high-water of THIS process
    pub forms: NormalForms, // unchanged from today
}
```

### 2. Functions, signatures first, body as pseudo-code comment

```rust
/// The whole matrix, one place, `'static`. Hand-listed, never a cartesian
/// product: a (lang, family, tier, oracle) whose oracle tsv does not exist is
/// not a case.
pub fn cases() -> &'static [Case];

/// The expensive half. Panics (never silently degrades) when the tier's
/// machinery is not compiled in.
pub fn run(lang: &str, tier: Tier) -> Run;
//  assert !cfg!(debug_assertions)                      // keeps mod.rs:758 rail
//  match (lang, tier):
//    (_,      Syntax)  -> rust_checker: None, ts_checker: None, ScipMode::Off
//    ("rust", Checker) -> assert cfg!(feature="rust-checker"); rust_checker: Some(root)
//    ("ts5",  Checker) -> assert cfg!(feature="ts-checker");    ts_checker: Some(root)
//    ("go",   Checker) -> panic!("go has no checker tier")
//    (_,      Scip)    -> panic!("the scip tier has no case yet")
//  files = enumerate(corpus(lang));  assert files.len() >= 500
//  3 runs, each under WALL_BUDGET_MS, median wall, peak rss, last run's forms

/// The cheap half. No extraction, no IO beyond loading the oracle tsv.
pub fn score_case(case: &Case, run: &Run) -> Score;
//  oracle = load_tsv(BENCH_DIR/case.oracle)
//  sides  = project(case.projection, run, case.family, oracle)
//  score(&sides.ours, &sides.oracle)          // unchanged: overlap, 3-bucket

/// The projections, ported verbatim from today's bodies; only the SELECTOR
/// changes. GoMethod/GoImpl call the existing `go_project`; RustCorpus calls
/// the existing `rust_project`; Raw clones both sides.
pub fn project(kind: Projection, run: &Run, family: &str, oracle: &BTreeSet<String>) -> Sides;

/// One (lang, tier) leg: one Run, every case that names it, then check or bump.
pub fn ratchet(lang: &str, tier: Tier);
```

`GoProjection::per_oracle` (mod.rs:291) and `RustProjection::per_oracle`
(mod.rs:441) are DELETED. `GoProjection` / `RustProjection` structs stay as the
parameter bags `go_project` / `rust_project` already take; `project()` builds
them from the `Projection` variant.

### 3. Storage layout, then reads and writes, then uniqueness

Two files, because one row states one fact. Accuracy is per case; cost is per
run; today's single file duplicates cost across four rust rows.

`plans/extract-bench-2026-08-29/RATCHET.tsv` (7 columns):

```
lang	family	tier	oracle	recall	precision	sha
```

`plans/extract-bench-2026-08-29/RATCHET.cost.tsv` (6 columns, NEW):

```
lang	tier	files	wall_ms	rss_mb	sha
```

| file | uniqueness | tolerance on check |
|---|---|---|
| RATCHET.tsv | one row per `(lang, family, tier, oracle)` | recall and precision, 0.10 pt (`RECALL_TOLERANCE_PT`, unchanged) |
| RATCHET.cost.tsv | one row per `(lang, tier)` | wall +15%, rss +10% (`WALL_TOLERANCE_PCT`, `RSS_TOLERANCE_PCT`, unchanged) |

Read order per leg: read both files, run the extraction, score every case that
names `(lang, tier)`, compare, then (bump only) rewrite both sorted. Sort keys:
`(lang, family, tier, oracle)` and `(lang, tier)`. `RATCHET_BUMP=1` and
`RATCHET_FORCE=1` keep exactly today's meaning (mod.rs:1090-1145), now applied
to both files.

Assert on read that no `(lang, family, tier, oracle)` repeats and no
`(lang, tier)` repeats; a duplicate key is a panic, not a last-wins.

### 4. Lifetimes

| value | born | dies | size |
|---|---|---|---|
| `cases()` | `'static` | never | one slice |
| `Run` | first line of `ratchet(lang, tier)` | end of that fn | the corpus's whole normal form (ts5 ~74k call rows) |
| oracle `BTreeSet` | inside `score_case` | end of that call | one tsv |
| `Score` | per case | printed, dropped | counts only |

One `Run` per test process. One test process per `(lang, tier)`, so `rss_mb` is
that pair's peak alone (the reason `ratchet_recall.rs:4-6` already splits by
lang).

### 5. Test entry points, `tests/ratchet_recall.rs`

Replace the three fns with one per leg, each `#[ignore]`:

```
ratchet_ts5_syntax   ratchet_ts5_checker
ratchet_go_syntax
ratchet_rust_syntax  ratchet_rust_checker
```

### 6. `v6/justfile`, recipe `extract-ratchet` (line 79)

ONE feature set for every leg, so cargo builds once:
`--release --features cli,rust-checker,ts-checker`. Five `cargo test` calls,
each `--exact --ignored --nocapture <fn>`, each still wrapped in `{{ capped }}
"${EXTRACT_RATCHET_BUDGET_S:-900}"`. Keep the trailing
`bench_normal_form` call unchanged. Update the recipe comment: name both tsv
files and say the legs are per `(lang, tier)`.

Note in the PR body whether compiling `rust-checker` into the ts5/go binaries
moved their `rss_mb`; if it did, that is the new honest ceiling, say the number.

## Acceptance receipts (all in the PR body, no exceptions)

1. **The python twin is reproduced.** `ratchet_ts5_checker` against
   `ts.codeql2.call.tsv` must land on recall `97.33` / precision `70.36` and
   against `ts5.oracle.call.tsv` on `95.02` / `76.73`, matching
   `plans/extract-crawl-2026-08-29/ts.REPORT.md:637` to 0.10 pt. Paste both
   lines. A miss here means the Rust leg and the python twin disagree; find out
   which is wrong and beep the coordinator BEFORE bumping anything.
2. **The syntax rows are byte-unchanged.** `ratchet_ts5_syntax` and
   `ratchet_go_syntax` reproduce today's `RATCHET.tsv` recall/precision for
   their oracles exactly (ts5 call/codeql2 `92.07 / 71.15`, ts5 call/oracle
   `88.20 / 76.13`, ts5 module `50.57 / 32.85`, go's four rows as committed).
   Any drift is a defect you introduced; stop and find it.
3. **rust checker rows hold** at `84.28 / 83.64` (codeql call),
   `93.66 / 51.82` (oracle call), `77.09 / 36.98` (scip_override call),
   `64.93 / 98.26` (type typedecl).
4. `rust_syntax` rows are NEW; plant them with `RATCHET_BUMP=1` and paste them.
5. `cd v6 && just extract-ratchet` rc=0 on a second, clean run against the
   committed files. Capture rc directly, no pipes (failure-mode 101/104: a
   piped rc and a dev-profile build both read as fake results).
6. The three unit tests already in `tests/bench/mod.rs`
   (`go_projection_drops_test_closure_and_iface_rows` :353,
   `rust_projection_drops_out_of_corpus_and_mirrored_closure_rows` :533,
   `three_buckets_partition_ours_and_split_silence_from_disagreement` :685)
   still pass, untouched in body.
7. `cargo test --release --features cli --test bench_normal_form` rc=0.
8. `git diff --stat origin/main...HEAD` lists only owned files.

## Files you own

- `v6/sprefa-extract/tests/bench/mod.rs`
- `v6/sprefa-extract/tests/ratchet_recall.rs`
- `v6/sprefa-extract/tests/bench_normal_form.rs` (only if the `mod bench;`
  surface forces it)
- `v6/justfile`, the `extract-ratchet` recipe and its comment ONLY
- `plans/extract-bench-2026-08-29/RATCHET.tsv`
- `plans/extract-bench-2026-08-29/RATCHET.cost.tsv` (new)
- `plans/extract-bench-2026-08-29/COMMON.md`, the "## Ratchet" section only
  (lines 39-50): say the key is `(lang, family, tier, oracle)`, name both files

FORBIDDEN: `v6/sprefa-extract/src/**` (this lane changes no resolver and moves
no accuracy number), every other file under `tests/`, `plans/extract-crawl-*`
(including `ts5.checker.measure.py`, which STAYS: it is receipt 1's reference
and a later PR retires it), the corpus checkouts, everything under `v6/prolog`
and `v6/tsv2`, root `src/`.

## Laws in force

- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers. The word is "oracle", never "ground truth".
- Comment budget: comments state constraints the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
  The receipt comments already in `mod.rs` (the FAIL-PRE-FIX and
  direction-convention headers) stay.
- No `eprintln!` in `src/**`; this lane touches no `src/**` anyway.
- Descriptive names, never single-letter.
- Every accuracy number you write anywhere carries the full measure signature
  of `COMMON.md:58`.
- Never `--no-verify`. A blocked command ends the approach; beep the
  coordinator.
- 10-second law: no foreground wait over 10s. The ratchet legs run under
  `{{ capped }}`; run them in the background with a log and poll.
- You are a lane: never spawn a subagent.

## Reaching the coordinator

```
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<one line: what, where, what you need>"
```

Blocked, done (with the PR number and the pasted rows), or brief-is-wrong.
