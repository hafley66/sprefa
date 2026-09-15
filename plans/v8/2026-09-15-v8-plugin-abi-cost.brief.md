# Brief: measured cost of the four ways to host a dl8 executor out of process or out of crate

## 1. Job
One document, `plans/v8/2026-09-15-v8-plugin-abi-cost.md`, under 120 lines, tables only, one mermaid. Four candidates for hosting an `IExecutor` (`v8/src/_9_runtime/_2_reconcile.rs:21-32`: `answer(pending) -> rows`, `poll(timeout) -> rows`) outside the `dl8` crate, each built as a scratch crate under `/private/tmp/claude-501/plugin-abi/`, each measured, none merged into `v8/`.

## 2. Base and first action
- Base sha: `54c739659537836dde4741afaad32f4e28a18e23` (`origin/main`). Branch `docs/v8-plugin-abi-cost`.
- FIRST command: `git merge --ff-only 54c739659537836dde4741afaad32f4e28a18e23`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 3. Ownership
You own the one doc file. Forbidden: every other path in the repo. Scratch crates live outside the repo. Never spawn subagents. Never `--no-verify`. Another lane builds the real dylib executor at the same time; do not touch `v8/src`.

## 4. Candidates, each a host binary plus a plugin that answers `pending = ["a","b"]` with rows `[["a","v1:a"],["b","v1:b"]]`
| id | mechanism | crates |
|---|---|---|
| P | separate process, JSONL over stdio | none (std only) |
| L | raw cdylib, C ABI, strings across, plugin frees its own strings | `libloading` |
| S | stable-ABI cdylib, the trait crosses | `abi_stable` (`#[sabi_trait]`, `RBox`), and `stabby` as a second row if it builds in under 30 min of effort |
| W | wasm component, the trait as a WIT interface | `wasmtime` + `wit-bindgen` |

## 5. Measure, per candidate, 3 runs each, numbers only from commands you ran
| measure | command |
|---|---|
| host cold `cargo build --release` wall s | `cargo clean && date +%s.%N && cargo build --release && date +%s.%N` |
| host incremental build s | `touch src/main.rs && ...` |
| unique crates in the host tree | `cargo tree --prefix none --no-dedupe \| grep -oE '^[a-z0-9_-]+ v[0-9.]+' \| sort -u \| wc -l` |
| host binary bytes | `stat -f %z` |
| plugin cold build s and artifact bytes | same commands in the plugin crate |
| call latency: host calls `answer` 10000 times, p50 and p99 ns | `Instant` in the host, print the two numbers |
| reload: swap plugin v1 for v2 while the host loops, wall ms from swap to first v2 row, and whether any v1 pointer had to be dropped first | host prints it |
| what crosses the boundary | one line: strings, `#[repr(C)]` structs, trait objects, or wasm memory |
| ABI risk | one line: what breaks when host and plugin are built by different rustc versions; test it by building the plugin with `+stable` and the host with `+nightly` if both toolchains exist (`rustup toolchain list`), else say untested |

## 6. Doc shape
TOC. Section 1: decision table, one row per candidate, the columns above, plus a verdict word. Section 2: one mermaid flowchart of how rows move for the winner. Section 3: what stays unmeasured and why. Every number carries its command. No prose paragraphs; a one-line caption under each table at most.

## 7. Style laws
No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support, honest, grounded, distill. No "here is / below is / the following". No praise, no hedging.

## 8. Reporting
Commit the doc, push, `gh pr create --base main` title `docs(v8): measured cost of four executor plugin ABIs`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "plugin abi cost: PR #<n>, cold build P/L/S/W = <s>/<s>/<s>/<s>, call p99 = <ns>/<ns>/<ns>/<ns>, reload ms = <>/<>/<>/<>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
