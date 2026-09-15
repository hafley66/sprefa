# Brief: extract in hafley-rs feeds dl8, end to end

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
Prove, through the real binaries, that `sprefa-extract` now living in `hafley-rs` still produces the three families dl8 needs (`tsi`, `scip`, `diet_scip`) and that `dl8 compile --tsi <jsonl>` consumes the TSI stream. One real-binary test in `v8/tests`, one small fixture corpus, one receipt table in the PR. Anything broken in the moved crate is reported, not fixed here, unless the fix is under 20 lines and inside `crates/sprefa-extract`.

## 2. Base and first action
- sprefa base sha: `BASE_SHA` (`origin/main`). Branch `test/v8-extract-tsi-e2e-20260914`, worktree under `/Users/chrishafley/projects/sprefa-wt/`.
- hafley-rs base sha: `68fe077911f66284bad6266e053253cd5a11faba` (`origin/main` of `hafley66/hafley-rs`). The sprefa worktree carries a gitignored sibling link `hafley-rs` at its root (boop-start makes it); confirm with `ls -la hafley-rs`, and if missing: `ln -s /Users/chrishafley/projects/hafley-rs-wt/extract-check hafley-rs`.
- FIRST command: `git merge --ff-only BASE_SHA` in the sprefa worktree. Failure = stop and report via `boop beep`.
- Commits end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## 3. Ownership
You own: `v8/tests/_16_extract_tsi.rs` (new), `v8/fixtures/extract/**` (new, a tiny TypeScript corpus of at most 3 files and a tiny Rust corpus of at most 2 files), `v8/README.md` (one section: how the extract binary is found). In hafley-rs: `crates/sprefa-extract/**` only for a fix under 20 lines, on branch `fix/extract-post-move-20260914`, its own PR.
Forbidden: every other path in both repos. Never spawn subagents. Never `--no-verify`.

## 4. What exists
| thing | where |
|---|---|
| the crate, moved by hafley-rs PR #63 | `hafley-rs/crates/sprefa-extract`, bin `extract` behind feature `cli` (`Cargo.toml:211`, `:233`) |
| family modes, `diet_scip` = parse-only heuristic resolve, `scip` = real indexer | `crates/sprefa-extract/src/bin/extract.rs:294-400` |
| TSI writer and types | `crates/sprefa-extract/src/tsi/{mod,sink,types,registry,semantic,ingest}.rs` |
| SCIP wire, indexers rust/ts/go, prost decode | `crates/sprefa-extract/src/scip.rs`, `scip_decode.rs`, `scip_rows.rs` |
| dl8 TSI loader: 36 row kinds, identity, graph install | `v8/src/_4_comptime/_0_load/_3_wire.rs`, `_4_identity.rs`, `_5_graph.rs` |
| `dl8 compile <file> --project <root> --tsi <jsonl>...` | `v8/src/bin/dl8.rs` `compile_cli` |
| the existing load oracle cases | `v8/oracle/load/`, `v8/tests/_5_load_oracle.rs` |
| removal of the old copy from sprefa | sprefa PR #743 |

## 5. Design (decided)
- The test locates the extract binary in this order: `$SPREFA_EXTRACT_BIN`, then `hafley-rs/target/debug/extract` relative to the sprefa root, then `cargo build -p sprefa-extract --features cli --bin extract --manifest-path hafley-rs/Cargo.toml` (run once, in the test, with output captured; a build over 60 s is reported as a defect in the PR with the number, never normalised).
- Three runs over the fixture corpus: `--family tsi`, `--family diet_scip`, `--family scip` (scip requires `scip-typescript` on PATH; if absent the scip case is `ignored` with the reason printed, never silently passing).
- The TSI JSONL from run one is passed to `dl8 compile fixtures/extract/main.dl7 --project fixtures/extract --tsi <jsonl>`; the test asserts exit 0 and that the compile output's `compiler_rows` contain at least one `tsi.symbol` row and one `tsi.edge` row naming a symbol from the corpus. Expected JSON frozen beside the fixture; comparison is exact on the rows that name the corpus, not on the whole output.
- Receipt table in the PR: family, rows emitted, wall ms, dl8 rows consumed.

## 6. Deliverables in order
1. Fixture corpus and `main.dl7` declaring the `tsi.*` relations it reads (copy the declarations from an existing load oracle case; cite which).
2. `_16_extract_tsi.rs` with the three runs and the compile assertion.
3. README section.
4. If the moved crate fails to build or a family emits zero rows: a hafley-rs PR with the under-20-line fix, or a `boop beep` naming the throw site if larger.

## 7. Validation
```bash
cd v8 && cargo test --locked --test _16_extract_tsi
cargo clippy --locked --all-targets -- -D warnings
cd ../hafley-rs && cargo test --locked -p sprefa-extract 2>&1 | tail -3   # report the numbers, do not fix failures outside section 3
```
Background every battery; per-case cap 10 s, indexer builds excepted and reported.

## 8. Style laws
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, ground truth, refusal. No em dashes. Descriptive names in the fixture corpus and the `.dl7`.

## 9. Reporting
PR title `test(v8): extract from hafley-rs feeds dl8 compile --tsi`. Then:
```bash
boop beep --no-wait --as <your-lane-name> sprefa-coordinator "extract e2e: PR #<n>, tsi <rows>, diet_scip <rows>, scip <rows|ignored>, dl8 consumed <rows>, hafley-rs sprefa-extract tests <pass>/<total>"
```
Blocked or brief wrong: same command, one line, stop. One lane, one task.
