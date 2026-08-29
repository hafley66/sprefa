# Brief: go scip CLI kinks (lane `fix-extract-go-scip-cli`)

Read `plans/extract-corpus-2026-08-28/COMMON.md`. Source of the kinks:
`plans/extract-crawl-2026-08-29/go.REPORT.md` Kinks table, rows 5 to 8.

## First action
```
git merge --ff-only 7712a40b83a726981a736ef5dda424cea4bf49e3
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-go-scip-cli sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree, never a
globally installed one.

## Ownership
Yours: `src/bin/extract.rs`, `src/scip.rs`, `src/scip_ensure.rs`,
`tests/56_scip_cli_kinks.rs`. Forbidden: everything under `src/lang/`,
`src/project.rs`, `src/types.rs`, `src/schema.rs`.

## The four kinks, REPRODUCE FIRST, fix only what reproduces
| kink | reproduce with | expected |
|---|---|---|
| `--scip-build` ignores `--scip-timeout`: killed at outer timeout, rc 124, zero rows, no `scip_skip` row | a go module fixture whose indexer is a script that sleeps 30s (put a fake `scip-go` first on PATH); `timeout 10 extract --family scip --scip-build --scip-timeout 2 <root>` | rc 0 within 3s, ONE `scip_skip` row with reason `timeout` |
| `--scip-build` runs `scip-go .` (root package only) | `src/scip.rs:276` GO_SPEC args already carry `./...`; verify the argv the fake indexer receives | if `./...` reaches argv, record "already fixed" with the receipt and write no code |
| `--scip-facts` rejects a directory PATH | `extract --scip-facts --project-root X --scip-index Y X` | `src/bin/extract.rs:388-390` already exempts scip-facts; verify, record, write no code if it passes |
| broken-pipe panic on `extract ... \| head` | `extract --resolve <82 files> \| head -1` | `src/bin/extract.rs:321-337` has an intercept; verify rc 0 and no panic text on stderr; write no code if it passes |

## Tests
`tests/56_scip_cli_kinks.rs`: one `#[test]` per kink that reproduces,
fail-first (commit the red test, then the fix). The fake indexer is a shell
script the test writes into a tempdir and prepends to PATH for the child
process only. Wall assert: the timeout test finishes under 5s.

## Receipt
Table in the PR body: kink / reproduced yes-no / fix commit or "no code".
Gate `cargo test --features cli --no-fail-fast` (background, log, poll), SUM
line. Push, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-go-scip-cli sprefa-coordinator "scip cli kinks: PR #N, <k> reproduced, <k> fixed, gate <p>/<f>"`.

## Laws (inline)
No em dashes. No `eprintln!` (`tracing` only). Comments state constraints
only. No `cargo fmt` outside files you own. Every `extract` call under
`timeout 10`. Never `--no-verify`.
