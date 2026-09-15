# Brief: lab, a dl8 executor hosted in a dylib, reloaded while the store keeps its rows

## 1. Job
Prove, through the real `dl8` binary, that an `IExecutor` can live in a separately built `cdylib`, be loaded at run time by relation name, answer rows into the SQLite store, and be replaced by a rebuilt dylib without restarting `dl8` or losing the rows already stored. Lab: the plugin crate and the test are the deliverable; nothing in the language changes.

## 2. Base and first action
- Base sha: `54c739659537836dde4741afaad32f4e28a18e23` (`origin/main`). Branch `lab/v8-dylib-executor`.
- FIRST command: `git merge --ff-only 54c739659537836dde4741afaad32f4e28a18e23`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`.

## 3. Ownership
You own: `v8/src/_9_runtime/_3_executors/dylib.rs` (new), the one new match arm in `v8/src/_9_runtime/_3_executors/mod.rs` `executors_for`, `v8/Cargo.toml` (add `libloading`, already in `Cargo.lock:2143` as a transitive), `v8/labs/dylib_echo/**` (new cdylib crate, NOT a workspace member), `v8/fixtures/hosts/4_dylib.dl7` (new), `v8/tests/_21_dylib.rs` (new). Forbidden: `v8/src/_0_read` through `_8_driver`, `v8/src/_9_runtime/_2_reconcile.rs` (the `IExecutor` trait stays as is), `v8/oracle/**`, `v7/`, `v6/`, `sqlite_ivm/`. Never spawn subagents. Never `--no-verify`.

## 4. What exists, with lines
| thing | where |
|---|---|
| the seam: `IExecutor { relation, cadence, answer(u, rows, pending) -> Vec<Row>, poll(u, timeout) -> Vec<Row>, armed }` | `v8/src/_9_runtime/_2_reconcile.rs:21-32` |
| `Cadence::{Once, Continuing}` | `_2_reconcile.rs:13-18` |
| registration by relation name, one match arm per executor, missing relation = diagnostic | `_3_executors/mod.rs:46-100` `executors_for` |
| the executor that already crosses a process boundary with JSONL rows: copy its row encoding | `_3_executors/extract.rs` |
| a host fixture: declare the relation as a product, seed one application, derive from it | `v8/fixtures/hosts/0_refs.dl7` |
| the real-binary test shape: scratch dir, `Command::new(dl8)`, `--db` | `v8/tests/_20_hosts.rs:1-70` |
| `Row` | `v8/src/_6_eval/_1_program.rs:154` |

## 5. Design (decided)
- ABI: C, strings only. The plugin exports three `extern "C"` functions and nothing else crosses the boundary:
```rust
#[no_mangle] pub extern "C" fn dl8_executor_relation() -> *const c_char;          // "dylib_echo"
#[no_mangle] pub extern "C" fn dl8_executor_answer(pending_json: *const c_char) -> *mut c_char; // JSON array of rows, same encoding extract.rs uses
#[no_mangle] pub extern "C" fn dl8_executor_free(s: *mut c_char);
```
  No Rust types, no `dyn Trait`, no allocator sharing: every string the plugin returns is freed by the plugin's `free`. This is why raw `libloading` is safe here without `abi_stable`.
- Host: `Dylib` implements `IExecutor` with `Cadence::Once`, `poll` returns empty, `armed` false. Path comes from `DL8_DYLIB_PATH` (one env var, one plugin, lab scope). `answer` checks the file mtime before every call; when it changed, `drop(self.library)` then `Library::new` again. No plugin-owned pointer may outlive the `Library`: convert every returned string to an owned `String` and free it before returning.
- Plugin v1 answers `(dylib_echo ?In)` with rows `(dylib_echo In "v1:<In>")`; v2 is the same source with `"v2:"`. The relation is declared in the fixture as `(: dylib_echo (* (: input text) (: output text)))`.
- The `unsafe` block count in `dylib.rs` is the number of `libloading` calls, each with a `// SAFETY:` line naming the C-ABI contract above.

## 6. Deliverables in order
1. `v8/labs/dylib_echo/` cdylib crate, `cargo build --release` produces `libdylib_echo.dylib`; a `V=v2 cargo build` env or feature switches the prefix.
2. `dylib.rs` + the `executors_for` arm + fixture.
3. Test `_21_dylib.rs`, real binary, in order: build plugin v1 into scratch; run `dl8 run --db scratch/store.sqlite fixtures/hosts/4_dylib.dl7` for one tick with `DL8_DYLIB_PATH` set; assert the store holds `(dylib_echo "a" "v1:a")`; build plugin v2 over the same path (mtime moves); run the next tick in the same process if `dl8 run` has a resident loop, else a second invocation with the same `--db` (say which in the PR); assert the store now holds both `"v1:a"` and `"v2:a"` rows, proving reload without losing rows. Then the negative: point `DL8_DYLIB_PATH` at a file that is not a dylib and assert one diagnostic naming the path, exit code nonzero, no panic.
4. Receipt table in the PR: step, command, rows in store, wall ms.
5. Measure: `cargo build --release` wall time of `v8` before and after adding `libloading` (3 runs each).

## 7. Validation
```bash
cd v8 && cargo test --locked --offline && cargo clippy --locked --offline --all-targets -- -D warnings
grep -rn "eprintln!" src/ | wc -l                       # 0
git diff --stat origin/main -- v8/oracle | tail -1       # empty
git diff --stat origin/main...HEAD                       # section 3 files only
```
Batteries in the background, 10 s per-test cap; a test over 10 s is reported by name, never waited out.

## 8. Style laws
Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth, refusal, support (say refCount). No em dashes. Interfaces carry `I`. Comment budget: constraints only; `// SAFETY:` lines are constraints. Follow each file's existing style. Lab files carry `labs/` in the path and die on landing: the PR body names which parts become permanent (the executor, the test) and which the coordinator deletes (nothing else, since the plugin crate is the test fixture).

## 9. Reporting
PR title `lab(v8): dylib executor with reload, C-ABI strings`, base `main`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "dylib lab: PR #<n>, reload proven <yes|no>, rows v1+v2 <k>, negative diag <yes|no>, cargo test <pass>/<total>, clippy 0, build delta <s>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
