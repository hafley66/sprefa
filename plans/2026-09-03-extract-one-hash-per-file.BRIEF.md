# brief: one blake3 per file in go and ts

Lane: `fix/extract-one-hash`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Every path below is relative to it unless it starts with `v6/` or `docs/`.

## ARCH row

`v6/prolog/ARCH.pl` `task(extract_one_hash_per_file, unbuilt, [extract_trail])`: go and ts hash every file twice (the extract cache key, then the door's own parse-cache key). #664 (`2ce437427`) removed a third. `tests/31_tracing.rs:210` pins `HASHES_PER_FILE = [("go", 2), ("ts", 2), ("rust", 1)]`. The fix moves go and ts to 1.

## Where the two hashes are

| # | site | why it hashes |
|---|---|---|
| 1 | `src/dispatch.rs:31` `CacheKey::new(content_id_of(content), ...)` | the extract cache key. Stays. |
| 2 (go) | `src/lang/go.rs:80` inside `go_parse_shared_keyed` | the tree-sitter parse cache key, computed again from the same bytes |
| 2 (ts) | `src/lang/ts.rs:1699` `ts_receivers::store_facts(content_id_of(self.content.as_bytes()), ...)` | the receiver-facts store key |

rust hashes once: read `src/lang/rust.rs` before changing anything so the fix matches whatever rust already does. The span that counts is on the primitive `src/types.rs:62` `content_id_of`; every call reaches it.

## Shape (type signatures first)

```rust
// src/dispatch.rs
/// The blob `dispatch` keyed this extraction on. Set for the duration of
/// `Source::extract` on the calling thread; a door that needs the id reads it
/// here rather than hashing the same bytes again. `None` when a door is called
/// outside `dispatch` (unit tests call `GoSource.extract` directly).
thread_local! { static EXTRACTING: RefCell<Option<ContentId>> }
pub(crate) fn extracting_blob() -> Option<ContentId>;
// pseudo, inside dispatch(): let id = content_id_of(content); EXTRACTING.set(Some(id.clone()));
//   let out = get_or_extract(key, || Arc::new(src.extract(path, content, mask))); EXTRACTING.set(None); out
//   (a guard struct that clears on drop, so a panic inside extract never leaks the id into the pool thread's next task)

// src/lang/go.rs:75  go_parse_shared_keyed(content) ->
//   let id = crate::dispatch::extracting_blob().unwrap_or_else(|| content_id_of(content.as_bytes()));
// src/lang/ts.rs:1699 ->
//   let blob = crate::dispatch::extracting_blob().unwrap_or_else(|| content_id_of(self.content.as_bytes()));
```

Lifetimes: `EXTRACTING` is per pool thread, set and cleared around one `src.extract`; nothing outlives the call. Uniqueness: the id set is the id `CacheKey` was built from, so the door's cache key and the extract cache key are the same value by construction.

If you find that threading the id through the `Source::extract` signature is cleaner (5 impls: `astgrep.rs:236`, `kotlin.rs:1608`, `go.rs:2654`, `rust.rs:3270`, `ts.rs:3778`; 4 callers: `dispatch.rs:33`, `cfg.rs:181`, `bin/extract.rs:871`, `lang/data/_0_source.rs:33`; tests `25_go_specifiers.rs`, `43_python_corpus_gaps.rs`), say so in the PR body with the reason and do that instead. Pick one; do not build both.

## Receipts

1. `tests/31_tracing.rs:210` `HASHES_PER_FILE` becomes `[("go", 1), ("ts", 1), ("rust", 1)]`. Rewrite the SABOTAGE RECEIPT comment at `:217` for the new pin: name the commit and the line whose revert makes go/ts read 2 again.
2. `cargo test --test 31_tracing` 3/3 runs green.
3. `cargo test --test 45_emit_throughput` 3 runs, load average pasted beside each (`failure-modes.md` 107: control at the budget's own commit, never main tip; the band is machine drift, do not move it).
4. `cargo test --test golden_parity` and the resolve goldens unchanged: `git diff --stat origin/main...HEAD` lists no golden.
5. Full battery in background per the 10-second law: `cargo test 2>&1 | tail -30` pasted.
6. `grep -rn "eprintln!" src/` no new lines.

## Ownership

Owned: `src/dispatch.rs`, `src/lang/go.rs`, `src/lang/ts.rs`, `src/lang/ts_receivers.rs`, `src/lang/rust.rs` (only if rust needs the same seam), `tests/31_tracing.rs`, and the 5 `Source::extract` impls plus their callers ONLY under the signature-change option.
Forbidden: `src/project.rs`, `src/tsi/**`, `src/wire.rs`, `src/bin/extract.rs` (except a signature-change compile fix), `tests/9*.rs`, `tests/10*.rs`, `v7/**`, `docs/failure-modes.md`, `v6/prolog/ARCH.pl`. Two other lanes own those right now.

## Style laws

No em dashes. Comments state only constraints the code cannot show; no change-log narrative, no PR numbers in comments (the SABOTAGE RECEIPT in the test header is the one exception). `tracing` only, no `eprintln!`. Descriptive names, no single letters. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Commit subject: `extract: one blake3 per file in go and ts`.

## Done

Push the branch, open the PR against `main` with the receipts pasted in the body, then:
`boop beep --no-wait --as fix-extract-one-hash sprefa-coordinator "one-hash PR #<n>: 31_tracing 3/3, throughput <s> <s> <s> @load <l>, battery <pass>/<total>"`.
