# Who hosts what

[Example](#example) · [The roster](#the-roster) · [http.fetch](#httpfetch) · [Rule](#rule) · [Receipts](#receipts)

## Example

`timer` and `http.fetch` in one run: the timer thread fires, a rule turns each fire into a poll, and the poll reads `http.fetch` for one url.

```dl7
{{#include ../probes/15_ghcacher.dl7}}
```

```console
$ bash book/show.sh run book/src/probes/15_ghcacher.dl7 --serve timer,fetch_json --max-ticks 3 | grep -v '^(fetch_json_error '
(effect timer ref(application(timer, [100 none])))
(effect fetch_json ref(application(fetch_json, ["http://127.0.0.1:9/repos/cli/cli" none])))
(timer 100 1)
(timer 100 2)
(Watch "http://127.0.0.1:9/repos/cli/cli")
(Poll "http://127.0.0.1:9/repos/cli/cli" 1)
(Poll "http://127.0.0.1:9/repos/cli/cli" 2)
(Failed "http://127.0.0.1:9/repos/cli/cli" 1 0)
(Failed "http://127.0.0.1:9/repos/cli/cli" 2 0)
ticks 3
exit 0
```

Regold pending: the probe still declares `fetch_json` locally, and that name left the roster, so the run above now exits `served_relation_no_executor(fetch_json)`; the probe adopts `http.fetch` from `@std/http` with the next fixture pass. Both executors run inside the `dl8` process. The url's port refuses the connection, so a fetch answers a status-0 row, and `Failed` then holds for every poll. [ghcacher](4_ghcacher.md) reads the same run as a gap list.

## The roster

Columns and answers per served name: [Executors](../11_executors.md). This table adds where each answer is computed and what wakes it.

| served name | where the work runs | cadence | wake source | error relation | source |
|---|---|---|---|---|---|
| `timer` | a thread in `dl8`, one per `Timer`, sending fires on an `mpsc` channel | Continuing | its own clock: the earliest due period (`timer.rs:52-79`) | none | `src/_9_runtime/_3_executors/timer.rs:29-38`, `:154` |
| `http.fetch` | one task per request on `dl8`'s shared tokio runtime; reqwest, 10 s client timeout, every pending request in flight at once | Once | an `effect` row; tasks signal the loop's wake and the settle loop drains | none: a transport failure is status 0 in the row | `src/_9_runtime/_3_executors/http_fetch.rs` |
| `http.serve` | one accept task per port on `dl8`'s shared tokio runtime; hyper, one task per connection | Continuing | an `effect` row to arm; each connection signals the wake when a request or a timeout lands | `http.serve_error` | `src/_9_runtime/_3_executors/http_serve.rs` |
| `git.refs` | `dl8`, through the `soopy` path dependency; `soopy::Refs` and `RepositoryWatcher` | Continuing | `RepositoryWatcher::recv_timeout`, degrading to a 1 s re-read when the watcher fails to open or closes | `git.refs_error` | `git_refs.rs:15-16`, `:33-63`, `:74-101`, `:210-237` |
| `git.history` | `dl8` through `soopy::RevisionGraph`, which spawns `git` children | Once | an `effect` row | `git.history_error` | `git_history.rs:24-45`; `hafley-rs/crates/soopy/src/_12_revision_graph.rs:114` |
| `fs.at` | `dl8` through `soopy::SourceTree::git_files`, which spawns `git ls-files` | Once | an `effect` row | `fs.at_error` | `fs_at.rs:24-44`; `hafley-rs/crates/soopy/src/_9_git_files.rs:74` |
| `extract` | a long-lived child process, the `sprefa-extract` binary, one `--resolve` child per process, one request per root and family over its stdin | Once | an `effect` row; the child's stdout reader task signals the wake per record | `extract_error` | `extract.rs:17-23`, `:107-190` |

Dependencies: `soopy` is a path dependency and `reqwest`, `hyper` are crates.io ones (`Cargo.toml`).

## http.fetch

| question | answer | line |
|---|---|---|
| which process sends the request | `dl8` itself | `http_fetch.rs` |
| which library | `reqwest`, one `Client` per executor | `http_fetch.rs:46-54` |
| what bounds one request | a 10 s client timeout | `http_fetch.rs:21`, `:52` |
| what a non-2xx status does | the row lands with its real status; the `fetch_json_error` rule in `@std/http` takes it | `http_fetch.rs:204-206`; `std/http.dl7` |
| what a transport failure does | status 0, message in `response_body`, empty `content_type` | `http_fetch.rs:24`, `:183-189` |
| request headers, method, body | columns 1-4 of the row: method defaults to GET, headers from the list, body text or blob handle | `http_fetch.rs:95-104` |
| response headers | `content_type` only | `http_fetch.rs:192-206` |
| how many at once | every pending request, one task each | `http_fetch.rs:110-124` |

## Rule

- Every executor is a Rust type compiled into `dl8`; `executors_for` is the only place a served name meets one (`src/_9_runtime/_3_executors/mod.rs:169-298`).
- `extract` is the one executor that spawns its own child process, one for the process's life, respawned once; `git.history` and `fs.at` call `soopy` in `dl8`, and `soopy` spawns `git`.
- Continuing executors are polled without blocking on the tick loop; the loop's idle branch is one bounded wait on the runtime's wake signal.

## Receipts

| claim | path | command |
|---|---|---|
| timer fires and numbering | `tests/_17_reconcile.rs:161-201` | `cargo test --test _17_reconcile` |
| fetch in flight: body, transport failure as status 0 | `tests/_17_reconcile.rs:203-267` (regold pending: the fixtures declare `fetch_json`) | `cargo test --test _17_reconcile` |
| refs snapshot then moved ref; history; files per revision | `tests/_20_hosts.rs:228-345` | `cargo test --test _20_hosts` |
| extract rows per family, and with no binary | `tests/_20_hosts.rs:407-466` | `cargo test --test _20_hosts` |
| the probe on this page compiles | `book/src/probes/15_ghcacher.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
