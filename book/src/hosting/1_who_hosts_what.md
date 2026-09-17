# Who hosts what

[Example](#example) · [The roster](#the-roster) · [fetch_json](#fetch_json) · [Rule](#rule) · [Receipts](#receipts)

## Example

`timer` and `fetch_json` in one run: the timer thread fires, a rule turns each fire into a poll, and the poll reads `fetch_json` for one url.

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

Both executors run inside the `dl8` process. The url's port refuses the connection, so `fetch_json` answers one error row, and `Failed` then holds for every poll. [ghcacher](4_ghcacher.md) reads the same run as a gap list.

## The roster

Columns and answers per served name: [Executors](../11_executors.md). This table adds where each answer is computed and what wakes it.

| served name | where the work runs | cadence | wake source | error relation | source |
|---|---|---|---|---|---|
| `timer` | a thread in `dl8`, one per `Timer`, sending fires on an `mpsc` channel | Continuing | its own clock: the earliest due period (`timer.rs:52-79`) | none | `src/_9_runtime/_3_executors/timer.rs:29-38`, `:154` |
| `fetch_json` | the tick loop thread in `dl8`: `ureq` blocking GET, 10 s global timeout, urls one after another | Once | an `effect` row | `fetch_json_error` | `fetch_json.rs:12-13`, `:29-59`, `:71-109` |
| `git.refs` | `dl8`, through the `soopy` path dependency; `soopy::Refs` and `RepositoryWatcher` | Continuing | `RepositoryWatcher::recv_timeout`, degrading to a 1 s re-read when the watcher fails to open or closes | `git.refs_error` | `git_refs.rs:15-16`, `:33-63`, `:74-101`, `:210-237` |
| `git.history` | `dl8` through `soopy::RevisionGraph`, which spawns `git` children | Once | an `effect` row | `git.history_error` | `git_history.rs:24-45`; `hafley-rs/crates/soopy/src/_12_revision_graph.rs:114` |
| `fs.at` | `dl8` through `soopy::SourceTree::git_files`, which spawns `git ls-files` | Once | an `effect` row | `fs.at_error` | `fs_at.rs:24-44`; `hafley-rs/crates/soopy/src/_9_git_files.rs:74` |
| `extract` | a child process, the `sprefa-extract` binary, one run per root and family, killed past 10 s | Once | an `effect` row | `extract_error` | `extract.rs:17-18`, `:37-61`, `:111-146` |

Dependencies: `soopy` is a path dependency and `ureq` a crates.io one (`Cargo.toml:18`, `:21`).

## fetch_json

| question | answer | line |
|---|---|---|
| which process sends the request | `dl8` itself | `fetch_json.rs:43` |
| which library | `ureq` 3, one `Agent` per executor | `Cargo.toml:21`, `fetch_json.rs:30-39` |
| what bounds one request | `timeout_global` of 10 s | `fetch_json.rs:12-13`, `:32` |
| what a non-2xx status does | the body is read, then an error row with the status | `fetch_json.rs:33`, `:52-54` |
| what a non-JSON 2xx body does | an error row, message `body is not json: ...` | `fetch_json.rs:55-58` |
| request headers, method, body | none: `agent.get(url).call()` | `fetch_json.rs:43` |
| response headers | not read | `fetch_json.rs:42-59` |
| how often one url is fetched | once per process per application | `src/_9_runtime/_2_reconcile.rs:44-47`, `:140-143` |

## Rule

- Every executor is a Rust type compiled into `dl8`; `executors_for` is the only place a served name meets one (`src/_9_runtime/_3_executors/mod.rs:61-100`).
- `extract` is the one executor that spawns its own child process; `git.history` and `fs.at` call `soopy` in `dl8`, and `soopy` spawns `git`.
- Continuing executors are polled on the tick loop; the loop splits one `IDLE_SLICE` across the armed ones (`_2_reconcile.rs:34-35`, `:157-167`).

## Receipts

| claim | path | command |
|---|---|---|
| timer fires and numbering | `tests/_17_reconcile.rs:161-201` | `cargo test --test _17_reconcile` |
| fetch body, non-2xx, non-JSON, closed port | `tests/_17_reconcile.rs:203-267` | `cargo test --test _17_reconcile` |
| refs snapshot then moved ref; history; files per revision | `tests/_20_hosts.rs:228-345` | `cargo test --test _20_hosts` |
| extract rows per family, and with no binary | `tests/_20_hosts.rs:407-466` | `cargo test --test _20_hosts` |
| the probe on this page compiles | `book/src/probes/15_ghcacher.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
