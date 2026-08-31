# SPEED.REPORT.md `fix-extract-speed` (2026-08-29)

Lane: go corpus wall from 11.9 s toward 3 s, RSS down, no fact lost.
Binary: `v6/sprefa-extract/target/release/extract`, built at the lane head.
Corpora per `COMMON.md`. Command: `extract --resolve --family call,type <files>`.

## 1. Receipt (3 runs per corpus, median)

| corpus | baseline wall | final wall | baseline RSS | final RSS | lines | identity |
|---|---|---|---|---|---|---|
| go (5,097 .go) | 11,910 ms | 10,520 ms | 625 MB | 601 MB | 172,745 | byte-identical after sort, 3/3 runs |
| ts (704 src/**.ts) | 1,730 ms | 1,230 ms | 445 MB | 454 MB | 149,657 | byte-identical after sort, 3/3 runs |
| rust (873 crates/*/src/**.rs) | 1,330 ms | 1,270 ms | 485 MB | 486 MB | 158,377 | byte-identical after sort, 3/3 runs |

Per-run rows: `tools/speedbench.sh <lang> <prefix>`; raw JSONL + `/usr/bin/time`
logs in `out/<prefix>.raw.N.jsonl` / `out/<prefix>.time.N.txt`.
Identity check: `sort before.raw.1 | cmp - <(sort <change>.raw.N)` per run, all corpora, all changes.

## 2. Phase split (go, baseline build, `DL_TRACE_SUMMARY=1`)

Wall 12,052 ms. Busy is summed span time across threads.

| phase | busy | note |
|---|---|---|
| extract_file (read + parse + per-file families) | 8,973 ms | parallel on `EXTRACT_POOL` (7 workers) |
| - parse | 3,524 ms | **10,190 parses for 5,097 files = 2 parses per file** |
| - family call | 2,734 ms | |
| - family df | 1,008 ms | 1.9 M df rows |
| - family cst | 737 ms | 6.8 M cst rows |
| - family type | 625 ms | |
| resolve_arm:call | 5,047 ms | was fully sequential over 5,097 files |
| resolve_arm:type | 47 ms | |
| index builds + TargetIndex + flatten + JSONL sort/emit | ~3,000 ms | remainder, sequential |

Instrumented timeline (`RUST_LOG=sprefa_extract=debug`): extract_file closes
over wall 0 to 5.3 s (only ~32% pool utilization), resolve 5.4 to 10.2 s, then
sort + emit.

Top self-time frames (go, `sample`, baseline; idle frames removed):

| self samples | frame |
|---|---|
| 25,786 | __psynch_cvwait (idle pool workers) |
| 1,558 | ts_tree_cursor_child_iterator_next |
| 1,037 | ts_parser_parse |
| 535 | _nanov2_free |
| 477 | ts_lex |
| 418 | ts_tree_cursor_goto_sibling_internal |
| 396 | ts_subtree_summarize_children |
| 393 | ts_tree_cursor_goto_first_child_internal |
| 381 | ts_stack_push |
| 378 | core::str::converts::from_utf8 |
| 374 | ts_language_next_state |
| 275 | nanov2_malloc |
| 273 | ts_lexer__do_advance |
| 270 | ts_stack_pop_count |
| 188 | ts_subtree_release |

## 3. Changes, each with a before/after row

### 3.1 Parallel per-file resolve (OWNED, kept)

`resolve_arm:call` was one sequential `inputs.iter()` loop. The seam pin
`cx.own: RefCell<Option<ContentId>>` made `ProjectCx` `!Sync`, so the loop
could not move onto `EXTRACT_POOL`. `ProjectCx.own` is now a thread-local
(`types::set_own`, `own_blob` reads it first and keeps the span-count fallback),
`reader` became `&(dyn Fn + Send + Sync)`, and the loop is
`EXTRACT_POOL.install(par_iter)`. 8 test files drop the dead `own:` field.

| corpus | before | after |
|---|---|---|
| go | 11,910 ms | 11,300 ms |
| ts | 1,730 ms | 1,690 ms |
| rust | 1,330 ms | 1,280 ms |

The go gain is capped by a lock convoy inside `go.rs` (section 4.1): parallel
resolve raised the `resolve_arm:call` span busy from 5.0 s to 24.1 s across
threads, ~19 s of it blocked in `__psynch_mutexwait`. Fixing that mutex is the
go lane's unlock for this change; the parallel loop is already in place.

### 3.2 mimalloc global allocator under `cli` (OWNED, kept)

`_nanov2_free` + `nanov2_malloc` + `_free` ~ 1,000/45,000 samples on go.
Note: tree-sitter's C side calls libc malloc directly, so `#[global_allocator]`
covers only the Rust-side allocations. Won on all three corpora, so `cli` now
enables it (feature `mimalloc` remains optional for library consumers).

| corpus | before | after (combined with 3.1) |
|---|---|---|
| go | 11,300 ms | 10,580 ms, RSS 630 to 621 MB |
| ts | 1,690 ms | 1,660 ms |
| rust | 1,280 ms | 1,300 ms |

### 3.3 `covering_def` without the per-call sort (OWNED, kept)

`types::covering_def` sorted the whole CallF bundle's def nodes on EVERY call
(a `Vec` alloc + driftsort per resolved call edge). On ts this was the single
hottest Rust frame (`driftsort ... covering_defs` 455 self samples). Replaced
with one linear pass, no allocation, identical tie-break order
(min length, then min (start, end), then node order).

| corpus | before | after |
|---|---|---|
| go | 10,580 ms | 10,520 ms |
| ts | 1,660 ms | 1,230 ms |
| rust | 1,300 ms | 1,270 ms |

### 3.4 Not done and why

- Index build (`IndexBag` OnceLock slots, `HashMap<String,_>` keys): ~0.1 s on
  the timeline; not a lever at this corpus size.
- JSONL serialisation: `serde_json` per row + sort is inside the ~3 s
  remainder; a manual-escaping BufWriter is a candidate but the measured
  `write` + serde frames are <100 samples; deferred.
- Pool utilization during extract_file (8.6 s busy over 5.3 s wall): the
  remaining serialization is inside the per-file parse path itself
  (ts_parser_parse + cursor walk), which is tree-sitter-bound, not
  scheduler-bound.

## 4. Cross-lane findings (NOT edited, diffs offered to the owning lanes)

### 4.1 go.rs:3037-3062, global mutex held across a full re-parse (go lane)

`go_file_facts` / `go_facts_of_path` lock one process-wide
`Mutex<HashMap<..>>` and call `go_parse_file_facts` (a FULL tree-sitter
re-parse of the file) while holding the lock; every resolve-side lookup
(`go_is_method_def` via `go_facts_of_path`, `.fields`, `.aliases`,
`go_method_on_type`) re-locks per query. Sequential resolve hid this; the
parallel loop (3.1) turned it into a convoy: `__psynch_mutexwait` 9,301/2 s
samples during resolve, `resolve_arm:call` busy 5.0 s to 24.1 s.

Suggested diff (lock only the map, parse outside the guard; consider
`RwLock` or sharding for the read path):

```rust
fn go_file_facts(blob: &ContentId, path: &str) -> Arc<GoFileFacts> {
    static CACHE: OnceLock<Mutex<HashMap<ContentId, Arc<GoFileFacts>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().unwrap_or_else(|p| p.into_inner()).get(blob) {
        return hit.clone();
    }
    let facts = Arc::new(go_parse_file_facts(path)); // re-parse OUTSIDE the lock
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    guard.entry(blob.clone()).or_insert(facts.clone()).clone()
}
```

Same shape for `go_facts_of_path` (path-keyed twin). Hot frames to re-measure
after: `GoModuleIndex::resolve_call_in_own_dir`, `go_is_method_def`,
`go_facts_of_path`, `go_method_on_type`.

### 4.2 Every corpus parses every file TWICE (all three lang lanes)

Parse span count = 2x file count on every corpus (go 10,190/5,097, ts
1,402/701, rust 1,746/873). The second parse is the module plane:
`project.rs` `read_inputs_inner(modules=true)` calls
`ts_resolve::module_facts` / `rust_modules::rust_module_facts` /
`go_modules::go_module_facts`, each re-parsing from bytes it is handed, while
`dispatch` already parsed the same bytes. Parse is 3.5 s busy of 12 s on go
(~1.7 s of it the duplicate pass).

Suggested shape: hand the module-facts fns the existing parse (ast-grep
`Source`/tree handle or a `ContentId`-keyed parse cache in
`project::read_inputs_inner`), so one parse serves dispatch + module facts.
That alone is worth ~1.5-1.7 s on go.

### 4.3 Cursor re-walk in the go per-file path (go lane)

`sample` during extract_file shows `ts_tree_cursor_child_iterator_next` +
sibling/first-child/reset/current_node ~ 2,400/45,000 samples, fed by many
separate walk passes per tree (`go_walk_import_specs`, `go_walk_field_types`,
`go_walk_fns`, `go_walk_call_defs`, `go_walk_receivers`, `walk_go_entities`,
`go_collect_file_facts`, `go_walk_call_sites`, `walk_type_decls`). One walk
per tree collecting all facts in one pass is the fix; the ts corpus shows the
same pattern is NOT the ts hot frame (its hottest frame was 3.3, now fixed).

## 5. Gate

`cargo test --features cli --no-fail-fast`: rc=0, 109/109 suites ok
(`/tmp/gate3.log` in-lane). No golden updated, no fact changed.

---

# 5. Section 5, `fix-extract-speed-2` (2026-08-29)

Lane: the go lock convoy (4.1) and the double parse (4.2), off PR #581's final
state (base 5f48d80c, go 10,520 ms). Same command, same corpora, same
identity check as sections 1-4.

## 5.1 Receipt (3 runs per corpus, median)

| corpus | #581 wall | final wall | #581 RSS | final RSS | lines | identity |
|---|---|---|---|---|---|---|
| go (5,097 .go) | 10,520 ms | 6,680 ms | 601 MB | 595 MB | 172,745 | byte-identical after sort, 3/3 runs |
| ts (704 src/**.ts) | 1,230 ms | 1,230 ms | 454 MB | 461 MB | 149,657 | byte-identical after sort, 3/3 runs |
| rust (873 crates/*/src/**.rs) | 1,270 ms | 1,280 ms | 486 MB | 496 MB | 158,377 | byte-identical after sort, 3/3 runs |

Per-change rows: `out/goorig` (base rebuild), `out/a1` (4.1 step 1: parse
outside the guard), `out/go_a2`/`out/ts_a2`/`out/rust_a2` (4.1 step 3 + go's
4.2: module plane builds the file facts on the shared parse and publishes
them; the resolve arms read the store instead of re-parsing).

## 5.2 Changes

### 5.2.1 go.rs: parse outside the guard, then the store (4.1)

Step 1 is 4.1's suggested diff verbatim (lock only the map, parse outside
the guard). Step 3 goes further: `go_modules::go_module_facts` now also
collects the resolve arms' `GoFileFacts` in the module plane's own pass
(`go_file_facts_of_source`), and `project.rs` publishes every corpus file's
facts into a process-global `GoFileFactsStore` (two RwLock maps, path-keyed
and blob-keyed) BEFORE the resolve loop starts. The read side is
single-writer-then-read-only, so the resolve arms take read guards with zero
contention; the old parse-and-cache fallback stays for library/test paths
with no module plane.

| go | wall (median) |
|---|---|
| base rebuild (#581 head) | 10,490 ms |
| 4.1 step 1 (parse outside guard) | 9,090 ms |
| 4.1 step 3 + 4.2 go (module plane publishes facts) | 6,680 ms |

The convoy frames (`__psynch_mutexwait` 9,301 samples in 4.1) are gone from
the profile; `resolve_arm:call` no longer serializes on the facts mutex.

### 5.2.2 Parse once (4.2)

go: the extract pass's tree-sitter parse is handed to the module plane
through a single-entry thread-local (`go_parse_shared`, dispatch parses and
stores, `go_module_facts` consumes on the same worker thread). Receipt:
tree-sitter parse spans 5,095 for 5,097 files (the 2-file delta is the
content-keyed extract cache hitting on duplicate content), down from
10,190. The `DL_TRACE_SUMMARY` "parse" row still shows 10,190 because it
aggregates the ast-grep cst engine's own parse per file, a separate parser
that 4.2 does not touch.

ts/rust: the module planes use DIFFERENT front ends than the extract pass
(oxc `ModuleRecord`, `syn::parse_file`), so parse-once needs a handoff in
the files this lane does not own. The seams are in (`ts_resolve.rs` /
`rust_modules.rs` at 73bdb1337): `ts_module_facts_from_parsed` +
`ts_stash_module_facts`, `rust_module_facts_from_parsed` +
`rust_stash_module_facts`, consumed on read with a path+content-id check,
falling back to the module plane's own parse. The two call sites in
`ts.rs` / `rust.rs` were offered to the coordinator for the owning lane.
Walls on those corpora are unchanged (go-only code path).

## 5.3 Gate and ratchet

`cargo test --features cli --no-fail-fast`: rc=0, 595 passed / 0 failed at
73bdb1337 (no fact changed; output byte-identical per corpus).

`just extract-ratchet` with `RATCHET_BUMP=1`: green, wall rows moved:
go 10,520 -> 5,083 ms, ts5 1,230 -> 901 ms, rust 1,270 -> 960 ms
(ratchet harness walls; recall/precision unchanged within the 0.10 pt
check, rust call row held at its old sha because its wall did not clear
the 10% bump margin).
