# Lane `fix-extract-rust-grind` (opus-max): the rust arm to codeql-class numbers, and a codeql-rust opponent row

Standing (RATCHET.tsv, projected, rust-analyzer `crates/**/src`, 873 files):
recall 70.2 / precision 43.3 vs `rust.oracle.call.tsv` (ra_ap_ide call
hierarchy), 78.8 / 42.4 vs `rust.scip_override.call.tsv` (rust-analyzer's
own scip index). Go reads 97/91 vs codeql, ts 92/71. Rust is the sad column
and it has no opponent number because codeql's rust extractor panicked on
the corpus (`TOOLS.REPORT.md` "Rust" table: `thread 'main' panicked at
library/core/src/str/mod.rs:861:21` in `rust/tools/index-files.sh`, twice).
User ask (2026-08-30 13:55): grind rust until it stops being sad. You have
the whole afternoon and the machine to yourself, one process at a time.

## Read first
`plans/extract-bench-2026-08-29/COMMON.md`, `RUST-PARITY.REPORT.md`,
`STUDY.human.unga.md` sections 1-4 and 8, `plans/extract-crawl-2026-08-29/rust.REPORT.md`
sections 16-21, `rust-excess-1.FIX.BRIEF.md`, `v6/sprefa-extract/src/lang/rust*.rs`,
`tests/bench/mod.rs` (`RustProjection`), `tests/ratchet_recall.rs`.

## First action
```
git merge --ff-only <BASE_SHA>
cd v6/sprefa-extract && nice -n 15 cargo build --release --features cli 2>&1 | tail -1
```

## Arc 1 (30 min cap): a codeql rust row
`codeql` 2.26.4 is installed (`brew`). The panic is a str-slice boundary in
the extractor's file indexer: find the file that trips it (bisect the file
list with `codeql database create --language=rust --source-root <dir>` over
crate subsets, or set `CODEQL_EXTRACTOR_RUST_...` options per
`codeql resolve languages` / `codeql resolve extractor --language=rust` help),
exclude it, build the db, run the call-graph query
(`plans/extract-bench-2026-08-29/` has the go/ts `.ql` files; port the
shape), normalize to `rust.codeql.call.tsv` (4 columns, COMMON.md), score
codeql vs both oracles with `rust.project.py` + `bench.py`, add the row to
RATCHET.tsv and TOOLS.REPORT.md. If 30 min pass without a db, write the
exact failing command and file in TOOLS.REPORT.md and move on.

## Arc 2: the leak (recall 70 -> as high as it goes)
`oracle - ours` after projection. Classify 300 (seed 7) with the
ra_ap_ide probe (`ra_ide_probe/`) where a text guess would do: receiver
typed through a trait method's return, generic instantiation, method on a
field of a struct from another crate, macro-generated def, `impl Trait`
return, closure param, assoc fn through `Self`, re-exported path, other.
Counts, two file:line each, the `rust*.rs` fn that owns it, into
`rust.REPORT.md` section 22. Then fix classes largest first, fail-first
each (`tests/7N_rust_<class>.rs`, fixture under
`tests/fixtures/rust_findings/<class>/`, HEAD failure in the header, commit
red then green), re-measure after each. Stop a class when its fix would need
a type checker (say so with the throw site).

## Arc 3: the excess (precision 43 -> as high as it goes)
`rust-excess-1.FIX.BRIEF.md` names the name-pick class (ctor / type name in
value position / wrong def). Same loop: classify, fix largest first,
fail-first, re-measure. A row we cannot bind becomes an `unresolved{reason}`
row, never a guess.

## Rules that end a lane if broken
One extract or codeql process at a time, `nice -n 15`, `timeout 120` on
extract, `timeout 900` on codeql db builds (background, log). Rust wall
stays under 10 s and RSS under 700 MB on the plain leg (RATCHET.tsv rss
column); `just extract-ratchet` after every class. Never edit the oracle
tsvs. No `cargo fmt` on files you do not own. No file over 1 MB in git.
Never run the full gate while a measurement is running.

## Deliver
One PR per arc (`fix/extract-rust-grind-1`, `-2`, `-3`, each branched from
the previous after its PR is posted so the coordinator can merge in order).
PR body: the table before -> after for both oracles (and codeql when arc 1
lands), classes fixed with counts, wall/RSS, gate summary
(`nice -n 15 cargo test --release --features cli`, background, log;
wall-ratio flakes rerun 3x isolated). Hail after each PR:
`boop beep --no-wait --as fix-extract-rust-grind sprefa-coordinator "rust grind arc N: PR #M, ra r/p a/b -> c/d, scip c/d, classes <names>, gate x/y"`.

## Ownership
`src/lang/rust*.rs`, rust tests and fixtures, `plans/extract-crawl-2026-08-29/rust*`,
`plans/extract-bench-2026-08-29/rust.codeql.call.tsv`, `rust.project.py`,
`TOOLS.REPORT.md` (rust table), `RUST-PARITY.REPORT.md`, RATCHET.tsv rust
rows (BUMP). NOT `go*`, `ts*`, `scip*`, `types.rs`, `project.rs`,
`tests/bench/mod.rs` beyond a rust-only helper.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
