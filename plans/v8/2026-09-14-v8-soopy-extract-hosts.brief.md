# Brief: soopy and extract executors for dl8 run

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
Four more executors behind the `IExecutor` seam so a dl8 program can watch an org of repositories and read code facts: `soopy_refs`, `soopy_history`, `repo_at` (the three v6 git families, in-process through the `soopy` crate), and `extract` (the `sprefa-extract` binary, one budgeted run per demand, rows projected into relations). Zero shell: soopy owns every git process; extract is a Rust binary launched directly. Real-binary tests over a throwaway git repository the test creates with soopy itself.

## 2. Base and first action
- Base sha: `BASE_SHA` (`origin/main`). Branch `feat/v8-soopy-extract-hosts-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- FIRST command: `git merge --ff-only BASE_SHA`. Failure = stop and report.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`. Commands run from `v8/`.

## 3. Ownership
You own: `v8/src/_9_runtime/_3_executors/{soopy_refs,soopy_history,repo_at,extract}.rs` (new), `v8/src/_9_runtime/_3_executors/mod.rs` (registration), `v8/src/bin/dl8.rs` (the `--serve` roster help text only), `v8/Cargo.toml` (the `soopy` path dependency only), `v8/tests/_20_hosts.rs` (new), `v8/fixtures/hosts/**` (new), `v8/README.md` (one section).
Forbidden: `v8/src/_9_runtime/_2_reconcile.rs` (the seam is fixed; a defect in it is reported with the line), `v8/src/_6_eval/**`, `v8/src/_5_reify/**` (emitter lane), `v8/src/_4_comptime/**`, `v8/oracle/**`, `hafley-rs/**` (report defects by throw site), `v7/`, `v6/`. Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| `IExecutor { relation, cadence, answer(u, rows, pending) -> Vec<Row>, poll(u, timeout), armed }`, `Cadence::{Once, Continuing}` | `v8/src/_9_runtime/_2_reconcile.rs:21-32` |
| the two shipped executors, registration, `dl8 run --serve timer,fetch_json --db --max-ticks` | `v8/src/_9_runtime/_3_executors/{timer,fetch_json,mod}.rs`, `v8/src/bin/dl8.rs:79-93` |
| the real-binary test pattern with a local listener | `v8/tests/_17_reconcile.rs` |
| v6 git executors: one `soopy::Refs` snapshot per root, `git_ref` and `git_tag` rows; history; repo_at; watch | `v6/sprefa-engine-rs/src/executors/{git_refs,git_history,repo_at,watch}.rs`, roster `v6/sprefa-engine-rs/src/hosts.rs:112-125` |
| v6 scip executors, in-process diet side, budgeted indexer side | `v6/sprefa-engine-rs/src/hosts.rs:121-123`, `v6/sprefa-engine-rs/src/executors/mod.rs:76` ("soopy owns every Git process") |
| `soopy` public API: `Refs`, `diff_refs`, `RevisionGraph`, `discover`, `open`, `GitWorktreeRoot`, `SourceTree`, `RepositoryWatcher`, `enumerate`, `hash_object` | `hafley-rs/crates/soopy/src/lib.rs:29-51` |
| locating and building the extract binary | `v8/tests/_16_extract_tsi.rs:70-131` (`SPREFA_EXTRACT_BIN`, then sibling target, then cargo build with the canonical manifest) |
| extract families and the row kinds they emit | `hafley-rs/crates/sprefa-extract/src/bin/extract.rs:294-400`, `src/scip_v5_rels.rs` |
| "hafley66 is an ORG, watched whole": the required set `instant`, `sprefa`, `hafley-rs`, `hafley-rxjs`; a single `repo(...)` seed is a defect | `CLAUDE.md` User decisions |
| build cost of a path dependency on `sprefa-extract`: 253 crates, 58.9 s clean | `plans/v8/2026-09-13-v8-tour.md` section 7 |

## 5. Design (decided)
| relation served | cadence | application | rows answered | source |
|---|---|---|---|---|
| `(soopy_refs ?Root ?Name ?Sha)` | Continuing | `?Root` ground | one row per ref in the snapshot; a later poll returns the `diff_refs` delta as new rows | `soopy::Refs`, `diff_refs` |
| `(soopy_history ?Root ?Sha ?Parent ?Seconds)` | Once | `?Root` ground, `?Sha` ground or free | the revision graph rows | `soopy::RevisionGraph` |
| `(repo_at ?Root ?Sha ?Path ?Blob)` | Once | `?Root ?Sha` ground | one row per file: path text, blob sha text | `soopy::SourceTree`, `enumerate`, `hash_object` |
| `(extract ?Root ?Family ?Kind ?Payload)` | Once | `?Root ?Family` ground | one row per emitted fact: `?Kind` is the fact's `kind` atom, `?Payload` the fact's JSON object as a text term | the extract binary, `--family <Family>` over `?Root`, JSONL on stdout |
- `soopy` is a PATH dependency (`hafley-rs/crates/soopy`); measure `cargo build` before and after and put both in the PR. `sprefa-extract` is NOT a dependency: the binary is located exactly as `_16_extract_tsi.rs:70-131` does, then launched with `std::process::Command`, stdout parsed line by line. Budget 10 s per run; over it the executor kills the child and answers one `(extract_error Root Family "timeout")` row.
- Rows are interned through the `Universe` like `fetch_json` does; a JSON object payload stays one text term (structured projection is a later lane, name it in the PR).
- `soopy_refs` polling: `RepositoryWatcher` when available, else a 1 s re-snapshot; say which shipped.
- Instance lifetimes: one executor per served relation for the run; a `soopy_refs` snapshot per root lives in the executor.
- Uniqueness: rows are set-inserted by the `Store`; a ref that moves produces a new row (retraction is the retraction lab's job, not this lane's).

## 6. Deliverables in order
1. `soopy_refs` with a test that creates a repo through soopy, commits twice, runs `dl8 run --serve soopy_refs --max-ticks 2`, asserts the ref rows.
2. `soopy_history` and `repo_at` with tests over the same repo.
3. `extract` with a test over `v8/fixtures/extract/corpus` (already in tree) asserting at least one row per family in {type, call, diet_scip}.
4. A fixture program `v8/fixtures/hosts/org.dl7` seeding the four required org roots as `repo` rows and deriving `(head_sha ?Root ?Sha)`; the test runs it with `--serve soopy_refs` against local clones only if `~/projects/{instant,sprefa,hafley-rs,hafley-rxjs}` exist, else `ignored` with the reason printed.
5. README section listing the executor roster with columns.

## 7. Validation
```bash
cd v8 && cargo test --locked && cargo clippy --locked --all-targets -- -D warnings && grep -rn "eprintln!" src/ | wc -l
git diff --stat origin/main -- v8/oracle | tail -1     # empty
```
Batteries in the background, per-case cap 10 s; the extract build is the one named exception and its seconds go in the PR.

## 8. Style laws
Comment budget. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal, support. No em dashes. Interfaces carry `I`. Descriptive names. `tracing` only.

## 9. Reporting
PR title `feat(v8): soopy and extract executors`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "hosts: PR #<n>, cargo test <pass>/<total>, clippy 0, soopy build delta <s>, extract rows type/call/diet <a>/<b>/<c>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
