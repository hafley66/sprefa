# brief: extract's trail, so a slow run explains itself from disk

Lane: `feature/extract-trail`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.

## Why (the incident this closes)

`docs/failure-modes.md` 107: #562 added a second blake3 over every file and nobody saw it for five days. The only instrument was a 5.5 s wall budget (`tests/45_emit_throughput.rs`), which reads the machine as much as the code, and the only trail was stderr, off by default. CLAUDE.md law: "Self-diagnosis before execution. The system answers why is it slow, what was it doing, from its own on-disk trail." Extract has no on-disk trail.

Infra is bought: `tracing` + `tracing-subscriber` (in tree), `hafley-observe` (in tree, `HAFLEY_LOG_FORMAT=json`, `RUST_LOG`), `rusqlite` 0.40 bundled (in tree), `libc::getloadavg` (already used at `tests/45_emit_throughput.rs:51`). Nothing bespoke beyond the span names and two tables.

## Where it is today

- `src/trace.rs`: `parse_span(lang, engine)`, `family_span(lang, family)` with `nodes/edges/sites` recorded on exit; `SummaryLayer` folds per (lang, family) `micros/files/facts`; table to stderr only under `DL_TRACE_SUMMARY=1` (`tests/31_tracing.rs`).
- `src/bin/extract.rs:782`: `--bench` prints one `eprintln!` line per file (unwaived; 4 unwaived `eprintln!` remain in that bin against the "eprintln never comes back" law).
- No span around: content hashing (`shape::content_id_of`, `go.rs:83`), `go_bind_plan_store` (`go.rs:1141`), the go chain walk (`go_chain_of`), the ts/rust syntax tsi pass (A4), the semantic walks (`tsi/semantic.rs`), the JSONL write (`extract.rs:710` `stream`), each resolve leg.
- No load average recorded beside any timing. No run row anywhere on disk.

## Type signatures

```rust
// src/trace.rs (extend; the file's existing style: free fns returning Span, Empty fields recorded on exit)
/// Closed. A phase not on this list is a compile error, never a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase { Hash, Parse, Family, BindPlan, Chain, TsiSyntax, TsiSemantic, Flatten, Write, ResolveLeg }
impl Phase { pub const fn as_str(self) -> &'static str; }

/// One phase over one file (or one leg over one project). `bytes`, `rows`,
/// `calls` are recorded on exit by the caller that holds them.
pub fn phase_span(lang: &'static str, phase: Phase) -> Span;          // debug_span!("phase", lang, phase = phase.as_str(), bytes = Empty, rows = Empty, calls = Empty)
pub fn record_phase(span: &Span, bytes: u64, rows: u64, calls: u64);

// SummaryLayer (existing) folds `phase` spans too:
struct PhaseRow { micros: u128, files: u64, calls: u64, rows: u64, bytes: u64 }
pub struct SummaryState { rows: Mutex<BTreeMap<(String, String), Row>>, phases: Mutex<BTreeMap<(String, Phase), PhaseRow>>, start: Instant, load_start: f64 }
impl SummaryState {
    pub fn render(&self) -> String;                 // existing table, plus a second table: lang | phase | files | calls | rows | bytes | ms
    pub fn snapshot(&self) -> RunSnapshot;          // everything the trail writes, borrowed out under the lock once
}
pub struct RunSnapshot { pub started: SystemTime, pub wall: Duration, pub load_start: f64, pub load_end: f64, pub families: Vec<FamilyRow>, pub phases: Vec<PhaseRowOut> }

// src/trail.rs (new)
pub struct Trail { conn: rusqlite::Connection }
impl Trail {
    pub fn open() -> Result<Trail, TrailError>;    // ~/.agent/dl6.db (the one db), CREATE TABLE IF NOT EXISTS the two tables below; busy_timeout 2s
    pub fn write(&self, run: &RunSnapshot, argv: &[String], git_sha: Option<&str>) -> Result<u64, TrailError>;  // one INSERT for the run, one multi-row INSERT for phases; returns run id
    pub fn recent(&self, n: usize) -> Result<Vec<RunReport>, TrailError>;   // the canned report: last n runs, phase rows joined
}
pub struct RunReport { pub id: u64, pub started: String, pub argv: String, pub wall_ms: u64, pub load_start: f64, pub load_end: f64, pub phases: Vec<(String, String, u64, u64, u64, u64, u64)> }
pub enum TrailError { Open(rusqlite::Error), Write(rusqlite::Error), Home }
// pseudo: write = BEGIN; INSERT extract_run; INSERT extract_phase rows (one statement, one insert_rows, never per-row); COMMIT.
// pseudo: recent = SELECT run JOIN phase ORDER BY run.__id DESC LIMIT n, grouped in Rust.
```

Instance lifetimes: `SummaryState` is `Arc`, born in `trace::install` (cli only), lives to process exit; `Trail` is opened once at exit inside `extract.rs` after the summary renders, written once, dropped. Spans live per file per phase; the layer folds them on close and holds no span.

## Storage layout (`~/.agent/dl6.db`, surrogate integer keys, per `.claude/skills/sql-relational-design`)

```sql
CREATE TABLE IF NOT EXISTS extract_run (
  "__id" INTEGER PRIMARY KEY, started_utc TEXT NOT NULL, git_sha TEXT, argv TEXT NOT NULL,
  wall_ms INTEGER NOT NULL, load_start REAL NOT NULL, load_end REAL NOT NULL, pid INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS extract_phase (
  "__id" INTEGER PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES extract_run("__id"),
  lang TEXT NOT NULL, phase TEXT NOT NULL, files INTEGER NOT NULL, calls INTEGER NOT NULL,
  rows INTEGER NOT NULL, bytes INTEGER NOT NULL, micros INTEGER NOT NULL,
  UNIQUE (run_id, lang, phase));
CREATE INDEX IF NOT EXISTS extract_phase_run ON extract_phase(run_id);
```

Writes: one run row + one batched phase insert per process, only when the trail is ON. Reads: `extract --trail [N]` (default 5) prints `Trail::recent` as a table to stdout. Uniqueness: `(run_id, lang, phase)`. `lang`/`phase` stay TEXT here because the row count per run is under 50 and the query surface is `sqlite3 ~/.agent/dl6.db` by hand; say so in a comment. A `dict_phase` is not needed at this size.

## When the trail is on

| switch | effect |
|---|---|
| `--bench` | summary table to stderr through `tracing::info!` (the `eprintln!` at `extract.rs:782` is deleted), trail row written |
| `DL_TRACE_SUMMARY=1` | same as today plus the phase table; trail written |
| `DL_TRAIL=0` | never write the trail, even with `--bench` (tests set this so goldens and CI do not touch `~/.agent`) |
| `--trail [N]` | print the last N runs and exit; conflicts with every extraction flag |
| default | silence, no disk, byte-identical stdout: `tests/31_tracing.rs::no_rust_log_means_no_stderr_byte` stays green |

## Spans to place (each is ONE `phase_span` enter around the existing call; no logic moves)

| phase | site | fields |
|---|---|---|
| Hash | `shape::content_id_of` callers: `go.rs:83`, and the ts/rust/kotlin/python equivalents (grep `content_id_of(`) | bytes = input len, calls = 1 |
| Parse | already `parse_span`; leave it, but fold it into the phase table as `Phase::Parse` | |
| Family | already `family_span`; fold as `Phase::Family` with rows = nodes + edges | |
| BindPlan | `go_bind_plan_store` and its builder | rows = plan entries |
| Chain | `go_chain_of` | calls = 1 per site, rows = steps |
| TsiSyntax | the A4 pass entry in `ts.rs` and `rust_type_edges.rs` | rows = facts pushed |
| TsiSemantic | `tsi::semantic::emit_semantic` | rows = facts, calls = coverage claims |
| Flatten | `wire::flatten_each` (already a `flatten` span with `facts`; convert to `phase_span(Flatten)`, rows = facts) | |
| Write | `extract.rs:710 stream`'s write loop | bytes = written, rows = lines |
| ResolveLeg | `project.rs`: one span per leg name where `ResolutionOrigin` is minted (`same_file`, `corpus_unique`, `module_plane`, `checker`, `scip`) | calls = sites asked, rows = answered |

Every field is a value the code already holds (the file's own rule at `trace.rs:1-2`). No string parsing, no formatting on the hot path: `record_phase` takes integers.

## The rail that would have caught #562 (load-independent)

`tests/31_tracing.rs` gains `phase_calls_per_file_are_pinned`: run `extract --family cst,type,call` with `DL_TRACE_SUMMARY=1 DL_TRAIL=0` over `tests/fixtures/go/`, `ts/`, `rust/` (3 files each); parse the phase table; assert per lang: `hash.calls == files`, `parse.calls == files`, `flatten.files == files`, `chain.calls == <the fixture's call-site count, pinned as a literal>`. A second hash per file fails this on any machine at any load. Header carries a SABOTAGE RECEIPT: revert `2ce437427` (#664) locally and show `hash.calls == 2 * files`.

## Tests, `tests/31_tracing.rs` (extend) and `tests/103_trail.rs` (new)

| case | expected |
|---|---|
| silence | default run: zero stderr bytes, no file under a fake `HOME` |
| phase table | `DL_TRACE_SUMMARY=1`: a row per (lang, phase) actually entered; no row for a phase not entered |
| calls pinned | the rail above, three langs |
| bench through tracing | `--bench` stderr contains the summary table and NO line matching the old `extract .* serial` format; `grep -c 'eprintln!' src/bin/extract.rs` drops by 1 and the 4 waived lines stay |
| trail written | `HOME=<tmp> extract --bench <file>`: `<tmp>/.agent/dl6.db` has 1 `extract_run` row, N `extract_phase` rows, `load_start > 0` |
| trail off | `DL_TRAIL=0 --bench`: no db created under the fake HOME |
| trail read | `HOME=<tmp> extract --trail 1` prints the run with its phases; rc=0 with an empty db prints `no runs` |
| goldens | `golden_parity`, `1_resolve_cli` byte-identical |

## Gate

```bash
cd v6/sprefa-extract && cargo test --features cli --test 31_tracing --test 103_trail --test golden_parity --test 1_resolve_cli 2>&1 | tail -3
cd v6/sprefa-extract && cargo test --features cli 2>&1 | tail -3
cd v6/sprefa-extract && HOME=/tmp/trail-probe cargo run -q --features cli --bin extract -- --bench --family type tests/fixtures/resolve/0_caller.ts 2>&1 | tail -4; sqlite3 /tmp/trail-probe/.agent/dl6.db 'select lang, phase, files, calls, rows, micros from extract_phase' ; rm -rf /tmp/trail-probe
```

## Files you own

`src/trace.rs`, `src/trail.rs` (new), `src/lib.rs` (`pub mod trail;`), `src/bin/extract.rs` (`--bench` line, `--trail`, trail write at exit), `src/wire.rs` (the flatten span only), `src/lang/go.rs` (span enters at hash, bind plan, chain), `src/lang/ts.rs`, `src/lang/rust.rs`, `src/lang/rust_type_edges.rs`, `src/lang/kotlin.rs`, `src/lang/python/*.rs` (hash and tsi span enters only), `src/tsi/semantic.rs` (one span), `src/project.rs` (leg spans only), `tests/31_tracing.rs`, `tests/103_trail.rs`, `tests/45_emit_throughput.rs` (only to read `load` from the same helper if you move it into `trace.rs`).

Forbidden: any logic change inside a phase; `src/tsi/{types,registry,sink,ingest}.rs`; `tests/fixtures/**`; `v6/tsv2/**`; `v6/prolog/**`; `v7/**`; `~/.agent/dl6.db` schema of any `__txt_*` table.

## Style laws

- No `eprintln!` in `src/**`; this arc deletes one. `tracing` only.
- Comments: constraints only. No dates, no PR numbers, no incident narrative in code.
- Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth.
- No em dashes.
- N+1: one `INSERT ... VALUES (...),(...)` for the phase rows, never a statement per row.
- No per-file allocation for span fields: `&'static str` lang and phase, integer fields.
- Every new pub type is declared in `src/trace.rs` or `src/trail.rs` with its doc line; no bare helper structs in the bin.

## Done

PR titled `extract: phase spans, load beside every timing, the run trail in dl6.db, --trail`.
Then: `boop beep --no-wait --as <your-lane> sprefa-coordinator "trail PR #<n>: 31_tracing N, 103_trail N, hash.calls==files pinned for go/ts/rust, eprintln count 8->7"`.
