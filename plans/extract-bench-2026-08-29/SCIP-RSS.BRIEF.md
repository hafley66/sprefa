# Lane `fix-extract-scip-rss` (glm53f): scip-informed resolve under 700 MB and 10 s on every corpus

User rule (2026-08-30): a scip-informed run at 48 s and 2.2 GB RSS is a
defect. PR #589 got walls to ts 3.4 s, rust 5.0 s, go 24 s and RSS to
753 / 974 / 1,247 MB. Ceilings for this lane, per corpus, 3 runs each,
`nice -n 15`, `/usr/bin/time -l`: wall under 10 s AND peak RSS under
700 MB (plain resolve reads 623 MB on go, RATCHET.tsv rss column). Read
`plans/extract-bench-2026-08-29/SCIP-SPEED.REPORT.md` sections 2 and 4
first; it names both frames.

## First action
```
git merge --ff-only a60e11e942d49578a3eab1dda559fefd2f8e65d6
cd v6/sprefa-extract && nice -n 15 cargo build --release --features cli 2>&1 | tail -1
```
Baseline jsonl for identity: build origin/main's binary once in a separate
target dir (`CARGO_TARGET_DIR=/tmp/base-target`) or reuse the one #589 left
under `/tmp` if present, and keep its sorted jsonl per corpus. Index paths and
the driver: `plans/extract-bench-2026-08-29/out/scip_runs.sh`.

## Task 1: intern the symbols (the RSS frame)
`src/types.rs:1943` `ScipOccurrence { symbol: String, .. }` and its two
siblings at :1986 and :2009 hold 952,588 duplicated symbol strings on go
(75 MB of strings, more in the per-doc caches). Replace with a `SymbolId(u32)`
into one interner built at decode (`src/scip_decode.rs`), per-document sorted
`Vec<(start, end, SymbolId)>`, and a `HashMap<SymbolId, def_site>`. The
resolve arms (`src/lang/{go,ts,rust}.rs`, `src/lang/python/_0_source.rs`)
read symbols through one accessor `ScipIndex::symbol(&self, id) -> &str`;
no arm keeps its own String copies. `site_occurrence` (`scip.rs:730`) and
`definition_of` (`scip.rs:768`) keep their names, return ids. Drop the flat
per-occurrence twin entirely once the arms read the vectors.

## Task 2: the go wall frame
`types::containing_def_site` (`types.rs:1809`), 6,783 of ~14,500 on-CPU
samples on go. Make it a binary search over a per-file sorted def-span
vector built once per file (or an interval index), never a linear scan per
site. Profile before/after with `samply` or `cargo flamegraph` on the go
corpus and paste the top 5 frames in the report.

## Receipt per task (commit after each)
Table: lang, wall x3, peak RSS, both ceilings pass/miss. Identity: sorted
jsonl `cmp` against the baseline on all three corpora, on every commit.
`RATCHET_BUMP=1 just extract-ratchet` at the end (walls and rss may only
improve; recall/precision must be identical). If a ceiling still misses
after both tasks, name the next frame with sample counts and post.

## Task 3: failure-mode entry
Append entry 99 to `docs/failure-modes.md` in the file's existing shape
(incident, RCA, fail-pre-fix test, rail, entry): incident = scip-informed
`--resolve` at 48 s and 2.2 GB RSS, RCA = per-call RandomState cache keys
(every lookup missed) + whole-Index protobuf decode resident beside the flat
twin + per-occurrence String symbols, rail = the RSS and wall ceilings as
RATCHET.tsv columns checked by `tests/ratchet_recall.rs` on the informed leg.

## Ownership
`src/types.rs` (scip types only), `src/scip*.rs`, the scip-reading code in
`src/lang/go.rs`, `ts.rs`, `rust.rs`, `python/_0_source.rs` (no other edits
in those files), `tests/scip_freshness.rs`, `tests/8_scip_families_cli.rs`,
`tests/74_scip_relationship_family.rs`, `SCIP-SPEED.REPORT.md` (sections 5+),
`docs/failure-modes.md` (append only), RATCHET.tsv with BUMP only. NOT
`src/project.rs`, `tests/bench/mod.rs`, any go/ts/rust receiver or dispatch
logic. No `cargo fmt` on files you do not own. One extract run at a time,
`timeout 60`. Gate (`nice -n 15 cargo test --release --features cli`) in
background with a log, never during a measurement; wall-ratio flakes rerun
3x isolated. No file over 1 MB. Budget 90 min; past it, post with Task 1.

Push `fix/extract-scip-rss`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-scip-rss sprefa-coordinator "scip rss: PR #N, ts a s/b MB, rust c s/d MB, go e s/f MB, ceilings pass/miss, identity ok, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
