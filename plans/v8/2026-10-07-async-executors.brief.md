# Brief: async executors, async HTTP client, inbound HTTP server

Worktree: this directory (`$PWD`), branch `perf/async-executors`. Work only here.

## User rules for this lane (2026-10-07)

- Write and read code by eye. Do NOT run `cargo check`, `cargo build`, `cargo test`, or any test.
- Rust and dl7 only. No Python. No scripts committed.
- rustfmt never. Format by hand, matching each file's style. Do not reformat lines you did not write.
- Comments: only constraints the code cannot show. No dates, no change narrative.
- `eprintln!` banned in `src/**`; `tracing` only.
- Every loop/recursion you add is bounded (constant with a comment saying what it protects).
- Do not touch `sqlite_ivm/` or `hafley-rs/` (symlinks to other repos).
- Commit as you go, one commit per numbered item, plain subjects. No merges, no pushes.
- Owned paths: `src/_9_runtime/**`, `src/_7_effect.rs`, `src/bin/dl8.rs` (run loop only), `std/http.dl7` (new),
  `Cargo.toml` (dependencies only), `book/src/11_executors.md`, `book/src/hosting/**`.
  Forbidden: `src/_2_lower/**`, `src/_3_check/**`, `src/_6_eval/**`, `src/_8_driver/**`, `src/lib.rs`
  (another lane owns them).

## Today

- Executor trait: `answer(&mut self, u: &mut Universe, rows: &Store, pending: &[TermId]) -> Vec<Row>`,
  plus `cadence()` (`Once` / `Continuing`). Read `src/_9_runtime/_2_reconcile.rs` and
  `src/_9_runtime/_3_executors/mod.rs` first. All 9 executors block the tick inside `answer`.
- `fetch_json.rs:43-58`: one blocking `ureq` GET per pending url, serial; body via `read_to_string`;
  non-JSON body is an error row; 10 s `REQUEST_TIMEOUT`.
- `extract.rs:122-128`: spawns a subprocess (`Command::new(&binary)`) per `answer` batch.
- No inbound HTTP.
- `CLAUDE.md` style law: executor trait is Tower-named `IService` with `call` and `poll`.

## Library choice (build vs buy, recorded)

| need | candidates | pick | why |
|---|---|---|---|
| runtime | tokio, smol, async-std | tokio | reqwest and hyper run on it |
| HTTP client | reqwest, hyper-util client, ureq (sync) | reqwest | methods, headers, bytes bodies, timeouts, connection pool |
| HTTP server | hyper 1.x + hyper-util, axum, actix-web, tiny_http (sync) | hyper + hyper-util | routing is done by dl7 rules over rows; no router layer needed |

## Items

1. Trait split. Rename the trait to `IService` with
   - `call(&mut self, u: &mut Universe, rows: &Store, pending: &[TermId])`: start work, return nothing.
   - `poll(&mut self, u: &mut Universe) -> Vec<Row>`: drain finished work as rows, never blocks.
   Work started in `call` runs on a tokio runtime owned by the reconciler (one multi-thread runtime per
   process). Tasks hold only plain data (String, bytes, numbers); terms are minted in `poll` on the
   tick thread, so `Universe` never crosses threads. Port all 9 executors. Synchronous ones may do
   the work inside `call` and buffer rows for `poll`.
2. Tick loop: evaluate -> `call` new effect rows -> `poll` every executor -> insert rows -> evaluate.
   When nothing is in flight and no new effect rows exist, stop (today's stop rule). When work is in
   flight, wait on the runtime for the next completion with a bounded timeout instead of spinning.
3. `fetch_json` -> `http.fetch` on reqwest, all pending requests in flight at once. Row shape:
   `(http.fetch ?Method ?Url ?Headers ?RequestBody ?Status ?ContentType ?ResponseBody)`. Keep
   `fetch_json` as a thin dl7 rule over `http.fetch` (GET, JSON check) so existing fixtures keep
   their meaning. Bodies: text content types as strings; other content types as a blob handle
   (`blake3` hex of the bytes, bytes kept in an executor-owned map keyed by that hash). Timeout 10 s,
   error rows as today.
4. `extract`: one long-lived child per process, JSONL requests on stdin, JSONL answers on stdout,
   read on a tokio task; respawn once on exit, then error rows.
5. Inbound HTTP: `http.serve` executor (`Continuing`) on hyper. `std/http.dl7` declares
   `(http.serve ?Port)`, `(http.request ?Id ?Method ?Path ?Query ?Headers ?Body)`,
   `(http.response ?Id ?Status ?ContentType ?Body)`. A goal on `http.serve` with the port bound binds
   the listener. Each accepted request becomes an `http.request` row with a fresh integer id; the
   connection task waits for the matching `http.response` row (read in `call` from `rows`), writes it,
   and the executor retracts the request row on the next `poll`. No response within 10 s -> 504 and
   retract. Blob-handle bodies stream from the blob map.
6. Book: update `book/src/11_executors.md` and `book/src/hosting/0_the_seam.md` prose for `call`/`poll`
   (no new numbers in prose; follow existing page style).

## Report back

Commit shas per item, files touched, and every place you were unsure whether the code compiles
(list them; the coordinator will type-check later).
