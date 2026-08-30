# Lane `fix-extract-speed` (glm53f): the go corpus wall from 9.1 s toward 3 s, RSS down, no fact lost

User word (2026-08-29): fast. `extract --resolve` over typescript-go
(5,075 files) is 11,969 ms median at #579 (over the 10-second law), 666 MB peak, and `sample` says
tree-sitter bound: `ts_tree_cursor_child_iterator_next` 1,576,
`ts_parser_parse` 1,113, `_nanov2_free` 585, `ts_lex` 515 (ORACLES.REPORT.md
section 14). The 10-second law is one second away and every new go leg
lands inside that wall. TypeScript-5.9 is 2.48 s / 409 MB, rust-analyzer
2.18 s / 536 MB.

## First action
```
git merge --ff-only 2423127ad687a8773f8c2f76acc987f159e83d70
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```

## Measure first, commit the profile
`samply` if installed (`cargo install samply`; if the install passes 5 min,
fall back to `sample <pid> 5 -file`), one profile per corpus, top 15 frames
by self time in a table in `plans/extract-bench-2026-08-29/SPEED.REPORT.md`.
Split the wall: parse, per-file facts, index build (`IndexBag` OnceLock
slots, `src/types.rs`), resolve arms, jsonl serialisation. Use the
`tracing` spans that exist (grep `info_span\|debug_span` in
`src/project.rs`); add spans where a phase has none.

## Then fix, in order of measured share, each with a before/after row
Candidates you verify against the profile rather than assume:
1. Parse runs on `EXTRACT_POOL` (`src/project.rs:563`); check the pool
   size, the per-file work granularity, and whether the cursor walk runs
   twice over one tree (the `ts_tree_cursor_child_iterator_next` count
   suggests a re-walk). One walk per tree, facts collected in one pass.
2. Allocation: `_nanov2_free` at 585 samples says many small frees; look
   for `String` clones on hot paths (`to_string()`, `format!` in per-node
   code), `Vec` growth without `with_capacity`, and per-node `HashMap`
   inserts that a `Vec` + one sort would replace. `cargo build` with
   `mimalloc` as the global allocator is a legitimate library answer; bench
   it before and after and keep it only if it wins on all three corpora.
3. Index build: any `HashMap<String, _>` keyed by owned strings that a
   `ContentId`/interned key replaces.
4. Serialisation: `serde_json` per row vs one `BufWriter` with manual
   escaping; measure, do not assume.

## Receipt
Three runs per corpus per change, median wall and peak RSS. The output
must be byte-identical after sorting (`sort <before> | cmp - <(sort <after>)`)
for every corpus at every step; a step that changes a fact is reverted and
reported. Gate `cargo test --features cli --no-fail-fast` in background with
a log. PR body: the phase-split table, the per-change table, the identity
receipt.

## Ownership
`v6/sprefa-extract/src/project.rs`, `src/types.rs`, `src/lang/mod.rs`,
`src/schema.rs`, `src/bin/extract.rs`, `Cargo.toml` (allocator only),
`plans/extract-bench-2026-08-29/SPEED.REPORT.md`. NOT `src/lang/go.rs`,
`go_modules.rs`, `rust*.rs`, `ts*.rs` (three lanes own them; if the hot
frame is inside one, report the frame and the fix as a diff in
SPEED.REPORT.md and hail the coordinator, do not edit). No `cargo fmt` on
files you do not own. No file over 1 MB.

Push `fix/extract-speed`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-speed sprefa-coordinator "speed: PR #N, go 9147->x ms, ts 2480->y, rust 2180->z, RSS, gate a/b"`.
Laws: no em dashes, no eprintln (tracing only), descriptive names, comments
only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal.
