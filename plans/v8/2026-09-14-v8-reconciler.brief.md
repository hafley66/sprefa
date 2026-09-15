# Brief: reconciler, timer source, fetch executor

## 0. Contents
1. Job
2. Base and first action
3. Ownership
4. What exists, with lines
5. Design (decided)
6. Deliverables in order
7. Validation
8. Style laws
9. Reporting

## 1. Job
`dl8 run <compile.json> --serve timer,fetch_json --db <file>` runs a program as a process: each tick evaluates, reads the `effect` rows on served relations, hands each to a Rust executor, inserts the answers as rows, persists the tick, and evaluates again until no executor has anything pending. Two executors: `timer` (a source, fires on its own clock) and `fetch_json` (answers a demand once, errors land as `fetch_json_error` rows). Zero shell. Every test runs the real binary against a real local HTTP listener and a real clock.

## 2. Base and first action
- Base sha: `BASE_SHA` (`origin/main`). Branch `feat/v8-reconciler-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only BASE_SHA`. Failure = stop and report via `boop beep`.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Every command runs from `v8/`.

## 3. Ownership
You own: `v8/src/_9_runtime/**` (new files `_2_reconcile.rs`, `_3_executors/` and edits to `mod.rs`), `v8/src/bin/dl8.rs` (the new `Run` subcommand only), `v8/Cargo.toml` (dependencies for the HTTP client and the test listener only), `v8/tests/_17_reconcile.rs` (new), `v8/fixtures/reconcile/**` (new).
Forbidden: `v8/src/_6_eval/**`, `v8/src/_2_lower/**`, `v8/src/_3_check/**`, `v8/src/_5_reify/**`, `v8/oracle/**`, `v7/`, `v6/`, `sqlite_ivm/`. Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| `effect(Relation, Application)` written on every evaluation of a goal on a served relation | `v8/src/_6_eval/_5_evaluate.rs:244-274` |
| `Program.served: HashSet<TermId>` | `v8/src/_6_eval/_1_program.rs:167-172` |
| `program_names`, `serve_relations` | `v8/src/_6_eval/_6_json.rs` |
| `eval_cli`: load store, `Evaluate::reduce`, `persist` per tick | `v8/src/bin/dl8.rs:142-240` |
| `Store`, `mark_all` (frontier = old rows) | `v8/src/_6_eval/_5_evaluate.rs:45-65` |
| `IRowStore` | `v8/src/_9_runtime/_0_store.rs:120` |
| the redux `Slice` shape and `reduce_then_apply` | `v8/src/_7_effect.rs` |
| v6 executor trait and roster: `IHostExecutor::run -> Vec<HostRow>`, `ExecutorCadence::{Once, Continuing}`, `/clock/tick`, `http.rs` | `v6/sprefa-engine-rs/src/hosts.rs:25-45`, `:112-125`, `v6/sprefa-engine-rs/src/executors/{clock,http}.rs` |
| the tick contract to port later: ghcacher golden, 5 ticks, `interval(300,N)` clock, host response commits at tick N, feedback at N+1 | `v6/tsv2/goldens/ghcacher_tick_golden/README.md`, `1_schedule.json` |
| effect brief: loading, failed and ready states are rules over `effect` and the relation's rows | `plans/v8/2026-09-14-v8-effect-demand.brief.md:11` |
| the `fetch_json_error` row convention, taken from astra's review | `chat_log/20260914.1.dl8-effects-settled-design-reviews-extract-move.md` Open Questions |
| zero shell law | `CLAUDE.md` "Zero shell in the engine" |

## 5. Design (decided)
```rust
// v8/src/_9_runtime/_2_reconcile.rs
/// One executor answers effect rows for one served relation.
pub trait IExecutor {
    fn relation(&self) -> &str;
    fn cadence(&self) -> Cadence;              // Once | Continuing
    /// Effect applications not yet answered; returns rows to insert (possibly for
    /// another relation, e.g. fetch_json_error). Blocking is fine: one call per tick.
    fn answer(&mut self, u: &mut Universe, pending: &[TermId]) -> Vec<Row>;
    /// Continuing executors: rows that arrived on their own clock since the last tick.
    fn poll(&mut self, u: &mut Universe) -> Vec<Row>;
}
pub enum Cadence { Once, Continuing }

pub struct Reconciler { executors: Vec<Box<dyn IExecutor>>, answered: HashSet<TermId> }
impl Reconciler {
    /// Runs ticks until a tick produces no new rows and no Continuing executor is armed,
    /// or `max_ticks` is reached. Each tick: rows.mark_all(); reduce; collect effect rows for
    /// served rels; answer/poll; insert; persist. Returns the tick count.
    pub fn run(&mut self, u, program, rows, store: Option<&mut dyn IRowStore>, max_ticks: usize, fx) -> usize;
}

// v8/src/_9_runtime/_3_executors/timer.rs
/// (timer ?Period ?Tick): a source. Period bound by a seed row `(timer 300 none)`-style? No:
/// timer is served with nothing bound; the program declares `(timer ?P ?T)` goals with ?P
/// ground in the rule body. The executor reads the effect application to learn ?P, arms a
/// std::thread timer per distinct period, and poll() returns (timer P k) for each fire.
// v8/src/_9_runtime/_3_executors/fetch_json.rs
/// (fetch_json ?Url ?Body): Once. Url ground from the application; Body is the response
/// text parsed with serde_json and interned as a term (objects as (obj key value) rows is
/// NOT built here: Body is the raw text term). Non-2xx or transport error: one
/// (fetch_json_error Url Status Message) row instead, and NO fetch_json row.
```
- HTTP client is BOUGHT. Write the candidate table (ureq, reqwest blocking, attohttpc, minreq) with size, blocking API, TLS, and pick in the PR; default pick `ureq` unless the table says otherwise. Test listener: `tiny_http` as a dev-dependency, or `std::net::TcpListener` with a hand-rolled two-line response; pick and say why.
- Instance lifetimes: `Reconciler` lives for the `run` subcommand; executors live with it; the timer thread is joined at exit.
- Storage: rows go through the existing `Store` and `persist`; no new tables. The `answered` set is rebuilt from the store's rows on start (an application with a matching data row is answered).
- Reads and writes per tick: one `reduce`, one pass over `effect` rows, N executor calls, one `persist` transaction.
- Uniqueness: one executor per served relation name; a served name with no executor is a `served_relation_unknown`-style diagnostic, exit 1.
- The 10-second law: `fetch_json` has a 10 s request timeout; over it is a `fetch_json_error` row, never a hang.

## 6. Deliverables in order
1. `IExecutor`, `Reconciler`, `dl8 run` wired to the same `--serve` and `--db` flags as `eval`.
2. `timer` executor. Fixture: a program with `(timer 1 ?T)` and a rule counting ticks; test runs the binary with `--max-ticks 3` and asserts three `timer` rows and the count row.
3. `fetch_json` executor. Fixture: a program with a seed url and `(fetch_json ?Url ?Body)`; test starts a local listener returning a JSON body, runs the binary, asserts the `fetch_json` row and the derived row; a second case returns 500 and asserts one `fetch_json_error` row and zero `fetch_json` rows; a third case points at a closed port.
4. `--db` continuation: run twice against the same db; the second run answers nothing and inserts nothing (COUNT test on INSERT statements via `sqlite3_total_changes` or the store's own counter).
5. README section for `dl8 run`.

## 7. Validation
```bash
cd v8 && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings && grep -rn "eprintln!" src/ | wc -l
git diff --stat origin/main -- v8/oracle | tail -1     # empty
```
Batteries in the background, per-case cap 10 s.

## 8. Style laws
Comment budget. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support (say refCount). No em dashes. Interfaces carry `I`. Async stays out: blocking Rust, one thread for the timer. Descriptive names in `.dl7` fixtures.

## 9. Reporting
PR title `feat(v8): reconciler with timer source and fetch_json executor`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "reconciler: PR #<n>, cargo test <pass>/<total>, clippy 0, http client <pick>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
