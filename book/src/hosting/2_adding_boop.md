# Adding boop

[Example](#example) · [The files to add](#the-files-to-add) · [Where the executor runs](#where-the-executor-runs) · [Namespaced](#namespaced) · [Receipts](#receipts)

Not built. The page is the plan; no code for `boop_lane` exists.

## Example

A program that reads the lanes boop records. It compiles; serving it stops at construction, because no match arm names `boop_lane`.

```dl7
{{#include ../probes/13_boop_lane.dl7}}
```

```console
$ bash book/show.sh compile book/src/probes/13_boop_lane.dl7
(Watch "sprefa-coordinator")
exit 0
```

```console
$ bash book/show.sh run book/src/probes/13_boop_lane.dl7 --serve boop_lane
diagnostic served_relation_no_executor(boop_lane)
exit 1
```

The rows would come from boop's SQLite store at `~/.agent/boop.db` (`hafley-rs/crates/boop-store/src/lib.rs:1-3`, `:67-68`). One lane is one `agent_lane` row; its name is a `dict_session` value:

```console
$ sed -n 4269,4282p ../hafley-rs/crates/boop-store/src/ident.rs && sed -n 290,291p ../hafley-rs/crates/boop-store/src/_0_session_graph.rs
CREATE TABLE IF NOT EXISTS agent_lane (
  spawn_id INTEGER PRIMARY KEY,
  lane_id INTEGER NOT NULL,
  trace_id INTEGER,
  harness_id INTEGER,
  branch_id INTEGER,
  cwd_id INTEGER,
  model_id INTEGER,
  parent_lane_id INTEGER,
  goal TEXT,
  brief_path_id INTEGER,
  brief_markdown_id INTEGER,
  spawned_ts INTEGER NOT NULL
);
                       FROM agent_lane lane_row
                       JOIN dict_session lane ON lane.id = lane_row.lane_id
```

```
step 0  tick 0  (boop_lane "sprefa-coordinator" ?Branch ?Goal ?SpawnedTs)  -> effect row, application ["sprefa-coordinator" none none none]
step 1  tick 1  BoopLane::answer opens boop.db read-only                   -> SELECT over agent_lane JOIN dict_session, dict_branch
step 2  tick 1  one row per spawn of that lane, or one boop_lane_error row  -> insert
step 3  tick 1  evaluate                                                   -> (Lane "sprefa-coordinator" Branch)
step 4  after 1 no new effect rows, nothing armed                         -> stop
```

## The files to add

`repo_at` is the nearest shape: Once, one text key bound, many rows, one error relation.

| step | file | what | analogous line in `repo_at.rs` or `mod.rs` |
|---|---|---|---|
| 1 | `src/_9_runtime/_3_executors/boop_lane.rs` | `RELATION = "boop_lane"`, `ERROR = "boop_lane_error"` | `repo_at.rs:10-11` |
| 2 | same | struct with the two relation ids and `new` | `repo_at.rs:13-22` |
| 3 | same | the read: open the store read-only, one `SELECT` joining `agent_lane` to `dict_session` and `dict_branch` for the bound lane | `repo_at.rs:24-44` (`files_at`); `boop-store/src/ident.rs:685-690` `Store::open_readonly`; join shape `boop-store/src/_0_session_graph.rs:288-298` |
| 4 | same | `impl IExecutor`: `Cadence::Once`, `answer` reads the bound text at position 0 and pushes rows or an error row | `repo_at.rs:46-92`; `mod.rs:28-37` `text_at` |
| 5 | same | `poll` empty, `armed` false | `repo_at.rs:94-100` |
| 6 | `src/_9_runtime/_3_executors/mod.rs` | `#[path]` module line and `pub use` | `mod.rs:7-8`, `:22` |
| 7 | same | the name in the error-bearing arm, its error constant, its constructor | `mod.rs:79-98` |
| 8 | `Cargo.toml` | `boop-store` as a path dependency, or `rusqlite` alone, already present | `Cargo.toml:15`, `:18` (the `soopy` path line) |
| 9 | `fixtures/hosts/5_boop_lane.dl7` | the probe on this page with a lane name from a scratch db | `fixtures/hosts/2_repo_at.dl7` |
| 10 | `tests/_20_hosts.rs` | a scratch `boop.db`, one lane row, `dl8 run --serve boop_lane`, assert `Lane` | `tests/_20_hosts.rs:228-345` |

Reading `boop-store` pulls its own dependencies (`boop-mux`, `sysinfo`, `time`, `dirs`: `hafley-rs/crates/boop-store/Cargo.toml:22-47`); reading the tables with `rusqlite` pulls none new and copies the join text instead.

A Continuing `boop_lane` needs a wake source for writes to `boop.db`. None of today's executors watches a plain file: [soopy continuous](3_soopy_continuous.md) lists the watchers.

## Where the executor runs

| shape | what crosses | call p50 ns | call p99 ns | host cold build s | reload ms | source |
|---|---|---|---|---|---|---|
| in process, a match arm (today's six) | `&mut Universe`, `TermId`, `Row` | not measured | not measured | not measured | no reload; rebuild `dl8` | `mod.rs:61-100` |
| child process, JSONL on stdio (P) | strings | 4833 | 13041 | 0.36 | 142.0 | cost doc row P |
| cdylib, C ABI with `libloading` (L) | `#[repr(C)]` C strings | 750 | 917 | 0.44 | 152.5 | cost doc row L; lab `dylib.rs` |
| cdylib, `abi_stable` (S) | trait objects | 208 | 250 | 9.51 | 146.1 | cost doc row S |
| cdylib, `stabby` (Y) | trait objects | 292 | 375 | 16.93 | 141.7 | cost doc row Y |
| wasm component, `wasmtime` (W) | canonical ABI | 625 | 750 | 61.81 | 12.6 | cost doc row W |

Cost doc: `plans/v8/2026-09-15-v8-plugin-abi-cost.md` section 1, on branch `docs/v8-plugin-abi-cost` (PR #766, commit `5eb4bfb45`). Lab: `src/_9_runtime/_3_executors/dylib.rs` on branch `lab/v8-dylib-executor` (PR #765), one more arm in `executors_for` reading `DL8_DYLIB_PATH`. Neither is on this book's base.

## Namespaced

| spelling | compiles today | serves today | what it would touch |
|---|---|---|---|
| `boop_lane` | yes, `probes/13_boop_lane.dl7` | no arm | steps 1 to 10 |
| `boop.lane` | no, it reads as a path: `unresolved_name(boop)`, `probes/19_dotted_name.dl7` | not reachable | steps 1 to 10 with a module `boop` exporting `lane` |
| a module `boop` exporting `lane` | the module graph exists, the name table is flat | no | [Namespacing](../modules/4_namespacing.md) forks C, D, F |

## Receipts

| claim | path | command |
|---|---|---|
| no arm names `boop_lane` | `src/_9_runtime/_3_executors/mod.rs:100` | this page's `run` console block |
| the lane table and the name join | `hafley-rs/crates/boop-store/src/ident.rs:4269-4282`, `_0_session_graph.rs:288-298` | this page's `sed` console block |
| the probes compile | `book/src/probes/13_boop_lane.dl7`, `19_dotted_name.dl7` | `cargo test --test _22_book probes_compile_as_their_page_says` |
| the ABI costs | PR #766 `plans/v8/2026-09-15-v8-plugin-abi-cost.md` | `git show 5eb4bfb45:plans/v8/2026-09-15-v8-plugin-abi-cost.md` |
