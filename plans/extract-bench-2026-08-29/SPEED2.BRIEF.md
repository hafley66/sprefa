# Lane `fix-extract-speed-2` (glm53f): the go lock convoy and the double parse

Read `plans/extract-bench-2026-08-29/SPEED.REPORT.md` sections 3, 4.1 and
4.2 (PR #581). After #581 the go corpus wall is 10,520 ms (over the
10-second law), ts 1,230 ms, rust 1,270 ms. #581 named two wins it did not
own; you own them now.

## First action
```
git merge --ff-only <origin/main sha the coordinator states; if #581 is merged this is after it>
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
bash plans/extract-bench-2026-08-29/tools/speedbench.sh   # the receipt tool #581 wrote; read it first
```

## Task A: go.rs lock convoy (SPEED.REPORT.md 4.1)
`go_file_facts` / `go_facts_of_path` (`go.rs:3037-3062`) hold a global
`Mutex<HashMap>` across a full tree-sitter re-parse; the parallel resolve
loop from #581 turns it into a convoy (`__psynch_mutexwait` 9,301 samples,
`resolve_arm:call` 5.0 s -> 24.1 s busy). Apply the diff in 4.1: parse
outside the guard, insert under the guard, `RwLock` or a sharded map for
the read path. Then the bigger fix: those facts are computed once per file
already by the module plane (`go_modules::go_module_facts`); the resolve
arms should read the module plane's `GoFileFacts` through `IndexBag`
(`go_modules` OnceLock slot) instead of re-parsing at all. Do the first
step, measure, then the second, measure.

## Task B: parse once (4.2)
`project.rs` `read_inputs_inner(modules=true)` hands bytes to
`go_module_facts` / `rust_module_facts` / `ts_resolve::module_facts`,
each re-parsing what `dispatch` already parsed (go 10,190 parses for 5,097
files). Pass the parsed tree (or the per-file facts `dispatch` produced)
into the module-facts fn; where the module plane needs nodes the dispatch
walk did not keep, collect them in that one walk. One parse per file per
language. Receipt: parse span count == file count on all three corpora.

## Receipt
Three runs per corpus per change via `speedbench.sh`, median wall and peak
RSS; `sort before.jsonl | cmp - <(sort after.jsonl)` identical for every
corpus at every step (a changed fact reverts the step). Gate
`cargo test --features cli --no-fail-fast` in background with a log; also
`just extract-ratchet` (PR #580) must stay green and you bump the wall rows
in `RATCHET.tsv` with `RATCHET_BUMP=1` when they improve. PR body: the
per-change table, the parse-count receipt, the identity receipt, gate.

## Ownership
`v6/sprefa-extract/src/lang/go.rs`, `go_modules.rs`, `ts_resolve.rs`,
`rust_modules.rs` (module-facts entry signatures only), `src/project.rs`,
`src/lang/mod.rs`, `plans/extract-bench-2026-08-29/SPEED.REPORT.md`
(append section 5), `RATCHET.tsv`. NOT `src/lang/rust.rs`,
`rust_receivers.rs` (a live lane owns them; if Task B needs a change
inside them, hail the coordinator with the diff). No `cargo fmt` on files
you do not own. Every extract invocation under `timeout 30`. No file over
1 MB.

Push `fix/extract-speed-2`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-speed-2 sprefa-coordinator "speed 2: PR #N, go 10520->x ms, parses 10190->y, gate a/b"`.
Laws: no em dashes, no eprintln (tracing only), descriptive names, comments
only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal.
