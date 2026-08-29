# Brief: CLI defects from the crawls (lane `fix-extract-cli-crawl`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` (style laws, 10-second law).
Findings come from `go.REPORT.md` in your tree; read its kinks table first.

## First action
```
git merge --ff-only c60e5c4cc
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as fix-extract-cli-crawl sprefa-coordinator "<one line>"`.
(If the build cannot find `../../../hafley-rs`, say so in the hail; the
coordinator adds the symlink.)

## Method
Every fix: failing test FIRST (red output pasted in the commit body), fix,
green, one commit per fix. Fixtures under `tests/fixtures/<lang>_findings/`
already exist for most rows; reuse them. Never weaken a golden or parity
test; regenerate `tests/fixtures/kind_vocab/wire_golden.jsonl` only by the
procedure `tests/6_kind_vocab.rs` documents and state the hunk count. Run
the gate as `cargo test --features cli --no-fail-fast` and report the SUM
over all binaries. No whole-crate `cargo fmt`. No subagents.

## Files you own
`v6/sprefa-extract/src/bin/extract.rs`, `src/bin/extract/help.rs`,
`src/scip_ensure.rs`, new `tests/50_cli_crawl_defects.rs`.
Forbidden: every `src/lang/**`, `src/project.rs`, `src/wire.rs`.

## Defects
1. `--scip-facts --project-root X --scip-index Y X` exits 2 "is a
   directory": `check_file_paths` (added in PR #532) runs for every mode
   but `--family scip`; `--scip-facts` and `--scip-deps` also take a ROOT.
   Read each mode's PATH contract in `help.rs` and exempt exactly the
   modes whose PATH is a root. Test: a dir arg under `--scip-facts` with a
   fake index reaches the library (rc != 2, no "is a directory" line).
2. `extract <file> | head -1` panics on broken pipe (`failed printing to
   stdout` or `Broken pipe` in stderr, rc 101). Handle `io::ErrorKind::BrokenPipe`
   from the BufWriter path AND the remaining `println!` rows (file fact,
   size_skip) as a clean exit 0 with nothing on stderr. Test: spawn the
   binary on a 200k-row input, read one line, drop the pipe, assert rc 0
   and empty stderr.
3. `--family scip --scip-build` runs `scip-go .` which indexes the root
   package only (158-byte index on typescript-go against 106 MB for
   `scip-go ./...`); switch the go invocation to `./...` (check the
   equivalent for the other indexers and leave them alone if they already
   walk the tree). Test: unit test on the argv builder.
4. `--scip-build` ignores `--scip-timeout`: the typescript-go build ran past
   the budget with no `scip_skip timed_out` row. Find where the deadline is
   applied (`scip_ensure.rs`) and why the go path skips it; test with a
   fake indexer script that sleeps past a 1 s budget and assert the
   `scip_skip` row with `reason: timed_out`.
5. `scip_skip.detail` on a failed rust-analyzer build carries `note: Some
   details are omitted, run with RUST_BACKTRACE=1` instead of the panic
   line before it (rust.REPORT.md kink 8). Pick the last stderr line that
   is not a `note:` line. Unit test on the line picker.

## Deliverables
Commits as above; append a Fixes table (kink / before / after / test) to
the report named at the top; push; `gh pr create --base main`; hail
`boop beep --no-wait --as fix-extract-cli-crawl sprefa-coordinator "fix-extract-cli-crawl: PR #N, <fixes>, gate <p>/<f>"`.
