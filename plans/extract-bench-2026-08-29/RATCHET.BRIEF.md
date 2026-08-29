# Lane `bench-extract-ratchet` (opus): recall, precision and wall ratchets over the committed oracles

User word (2026-08-29): "diet-scip ratcheted as high as possible so it is
fast". `diet_scip` is plain `--resolve` (`src/project.rs:491`). Nothing
today fails when a PR lowers recall against an oracle or raises the wall.
Build the ratchet.

## First action
```
git merge --ff-only 2423127ad687a8773f8c2f76acc987f159e83d70
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Deliverable, in Rust behind the library API (user word: "use what we
have behind our rust trait for this cli")
No python, no shelling to the binary. A cargo test binary
`v6/sprefa-extract/tests/ratchet_recall.rs` (ignored by default,
`cargo test --features cli --test ratchet_recall -- --ignored`) that calls
the same entry points the CLI calls: `resolve_project(&ResolveRequest)`
(`src/project.rs:170`) with the family mask the CLI builds
(`src/bin/extract.rs`, grep `parse_arms`), in-process, one call per corpus.
Normalisation lives in Rust too: a small `bench` module under `tests/`
(or `src/bench.rs` behind `#[cfg(feature = "cli")]`) that maps `FlatFact`
rows to the `src_path src_name dst_path dst_name` normal form exactly as
`normalize.py` does (port it; keep the python file as the reference and
add a test that the two agree on `go.parse.call.tsv`). Wall measured with
`Instant` around the call, RSS via `libc::getrusage` (dependency exists?
check Cargo.toml; else `mach` via the `memory-stats` crate, cite the
build-vs-buy line).
- measure: for each corpus in COMMON.md, 3 in-process runs, median wall
  and peak RSS, then recall and precision against every oracle tsv present:
  `ts5.oracle.call.tsv`, `ts.madge.module.tsv`, `go.oracle.call.vta.bare.tsv`,
  `go.oracle.module.tsv`, `go.oracle.type.typedecl.tsv`, `rust.oracle.call.tsv`,
  `rust.oracle.type.typedecl.tsv`, and against `go.codeql2.call.tsv`,
  `ts.codeql2.call.tsv` (the tools to beat). Print one table.
- check (the test's default assertion): fails if any recall or precision is below RATCHET.tsv
  by more than 0.1 point, or any wall is above its row by more than 15%, or
  RSS above by more than 10%. Prints the offending rows.
- bump (`RATCHET_BUMP=1` env): rewrites RATCHET.tsv to the measured values only where
  they improved (additive ratchet, never lowers a floor or raises a ceiling
  without `--force`).
- Columns: `lang family oracle recall precision wall_ms rss_mb measured_at_sha`.
- `just extract-ratchet` in `v6/justfile` (read it first, follow its style)
  runs the ignored test. Add the leg to `.github/CI-KNOWN-RED.md` ONLY if it cannot
  run in CI (the corpora are local paths; say so in one line in COMMON.md
  and mark the recipe local-only).
- If a corpus is absent the row reads `absent` and `check` skips it with a
  printed line, rc stays 0 for that row.

## Ownership
`v6/sprefa-extract/tests/ratchet_recall.rs`, `tests/bench_normal_form.rs`
(or `src/bench.rs`), `plans/extract-bench-2026-08-29/RATCHET.tsv`,
`COMMON.md` (one paragraph), `v6/justfile` (one recipe), `Cargo.toml`
dev-dependencies only. NOT `src/project.rs`, `src/types.rs`, `src/lang/*`,
`src/scip*.rs` (other lanes own them).

## Receipt
Commit RATCHET.tsv at the measured values of this sha. Push
`bench/extract-ratchet`, `gh pr create --base main`, hail
`boop beep --no-wait --as bench-extract-ratchet sprefa-coordinator "ratchet: PR #N, go call x% ts call y% rust call z%, walls a/b/c ms"`.
Laws: no em dashes, no words provenance/substrate/load-bearing/regime,
never "ground truth" (say oracle), every extract call under timeout 30.
