# extract-blob-cache (issue: extract-blob-cache-parallel, CACHE half only)

FIRST ACTION: `git merge --ff-only 924b8661fdd314bd0940bd9f9ddd2fba8b72cced`.
Failure or missing tree = STOP AND REPORT, do not work around it.
Then read `CLAUDE.md` at the repo root.

## GOAL

`sprefa-extract` re-parses a file every time it is asked for, even when the
bytes have not changed. Add the content-keyed extraction cache the crate's own
header has declared since day one, weight-bounded, concurrent, with a measured
hit that skips the parse.

This is the CACHE half of `@extract-blob-cache-parallel`. The PARALLEL half has
already landed; do not touch the rayon pool or the thread cap.

## DECIDED, do not relitigate

| decision | value |
|---|---|
| library | `quick_cache = "0.7"`, `quick_cache::sync::Cache` with `with_weighter` |
| why not moka / lru / dashmap / HashMap | full analysis in `plans/2026-08-17-extract-blob-cache-parallel.ANALYSIS.md`, already on main |
| bound | WEIGHT, never entry count. Default **512 MiB**, overridable by `SPREFA_EXTRACT_BLOB_CACHE_MB` |
| weight unit | an `ExtractOutput` byte estimate, not 1-per-entry |
| public signature | `dispatch` returns `Option<Arc<ExtractOutput>>`. Update EVERY call site in the same PR. No shim, no second entry point |

512 MiB sits above the 400 MB measured for holding this whole corpus resident,
so a full sprefa pass never evicts, and it is still a bound.

## RECEIPTS (verified at 924b8661f; re-check line numbers before you edit)

| fact | receipt |
|---|---|
| the function whose signature changes | `v6/sprefa-extract/src/dispatch.rs:14` `pub fn dispatch(path, content, mask) -> Option<ExtractOutput>` |
| the value type | `v6/sprefa-extract/src/types.rs:1735` `ExtractOutput` (owned, no lifetimes, NOT `Clone`) |
| the interner inside it, with PRIVATE fields | `v6/sprefa-extract/src/types.rs:91` `Strings { map, names }` |
| the per-family payload | `v6/sprefa-extract/src/types.rs:1075` `FamilyBundle { nodes, edges, aux }`, all `pub` |
| `FamilyMask` derives `Eq` but NOT `Hash` | `v6/sprefa-extract/src/types.rs:1710` |
| `soopy::ContentId` already derives `Hash + Eq` | `hafley-rs/crates/soopy/src/_0_types.rs:265` |
| the hash of the bytes | `v6/sprefa-extract/src/shape.rs` re-exports `content_id_of` |
| the declaration this closes | `v6/sprefa-extract/src/types.rs:2304-2305` |

CALL SITES, all six, all must move in this PR:

```
v6/sprefa-extract/src/project.rs:514 and :557 (read_inputs_plain / read_inputs_batched)
v6/sprefa-extract/src/bin/extract.rs:511
v6/sprefa-engine-rs/src/hosts.rs:1169
v6/sprefa-engine-rs/src/dep_resolve.rs:559
v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs:389
```

Both `project.rs` sites now sit inside a rayon `par_iter` closure. The cache
must therefore be safe to call from many workers at once, which is why
`quick_cache::sync` is the pick and a `Mutex<HashMap>` is not.

## WHAT TO BUILD

1. `quick_cache = "0.7"` in `v6/sprefa-extract/Cargo.toml` `[dependencies]`.
   Its two required deps (`equivalent`, `hashbrown`) are already resolved in
   both lockfiles, so this must add exactly one new package row.
2. A new file `v6/sprefa-extract/src/cache.rs` holding the whole cache: the key
   type, the weigher, the capacity rule, and the lookup.
   Key: `(soopy::ContentId, &'static str, u8)` where the `&'static str` is
   `Source::name()` and the `u8` folds `FamilyMask`'s four flags into bits.
   Deriving `Hash` on `FamilyMask` is ALSO acceptable and is the only other
   edit `types.rs` may receive (see below). Pick one and say why in the PR.
3. The weigher returns an `ExtractOutput` byte estimate: the interner's bytes
   plus, per present family, `nodes.len() * size_of::<Node<F>>()` and the same
   for `edges`. It does not have to be exact; it has to move with the real size.
   `Strings`'s fields are PRIVATE, so add ONE accessor to `types.rs`:
   `impl Strings { pub fn heap_bytes(&self) -> usize }`, placed immediately
   after the existing `Strings` impl block (which starts at `types.rs:96`).
   That plus the optional `Hash` derive at `:1710` are the ONLY `types.rs` edits
   allowed.
4. `dispatch` becomes: build the key from `content_id_of(content)` plus the
   matched `Source`'s name plus the mask, look up, and on a miss extract, wrap
   in `Arc`, insert, return. `quick_cache`'s `get_or_insert_with` is the
   intended call.
5. Capacity: `SPREFA_EXTRACT_BLOB_CACHE_MB` if set, parseable and nonzero, else
   512. Converted to bytes for `with_weighter`'s weight capacity. Estimated
   items capacity: derive it from the cap and a representative entry size, and
   say in the PR which number you used and why.

## WHAT NOT TO TOUCH

FORBIDDEN, any edit to these is a failed lane:

- `v6/sprefa-extract/src/lang/**` (another driver owns every language arm)
- every `df_*` and `doc_*` plane, anywhere, including inside `types.rs`
- `types.rs` beyond the two edits named above
- the rayon pool, `extract_thread_cap`, or anything the parallel half added
- `v6/prolog/**`, `v6/tsv2/**`, `v6/tools/**`, `.github/**`
- goldens and fixtures: regenerate nothing
- `v6/sprefa-engine-rs/Cargo.lock` beyond what the signature change forces

YOURS:

- `v6/sprefa-extract/Cargo.toml`, `Cargo.lock` (the quick_cache rows only)
- `v6/sprefa-extract/src/cache.rs` (new), `src/dispatch.rs`, `src/lib.rs`
- `v6/sprefa-extract/src/project.rs`, `src/bin/extract.rs`
- `v6/sprefa-extract/src/types.rs`, the TWO named edits only
- `v6/sprefa-engine-rs/src/hosts.rs`, `src/dep_resolve.rs`,
  `src/source_bind/_1_runtime.rs`, at their `dispatch` call sites only
- one new file `v6/sprefa-extract/tests/27_blob_cache.rs`

## TESTS

`tests/27_blob_cache.rs`, header carrying CONTROL and at least two SABOTAGE
rows with measured pass/fail splits, in the shape of
`v6/sprefa-extract/tests/25_query_digest_repo_from_path.rs`.

Required assertions:

1. HIT SKIPS THE PARSE, counted. Not an equality assertion: equal output proves
   nothing about whether the parse ran. Count the extractions (an
   `AtomicUsize` the test can read, or a `#[cfg(test)]` counter in `cache.rs`)
   and assert the SECOND call on identical bytes leaves the counter unmoved.
   This is the discriminating test and the repo asks for the count form
   explicitly.
2. TWO PATHS, ONE BLOB. Different paths with byte-identical contents and the
   same language hit the same entry; the counter moves once.
3. DIFFERENT MASK, DIFFERENT ENTRY. The same bytes at `FamilyMask::ALL` and at
   a narrower mask are two entries, and the second call parses.
4. EVICTION. With `SPREFA_EXTRACT_BLOB_CACHE_MB` set very small, insert past
   the cap and assert an earlier entry is gone (its next lookup moves the
   counter). The bound has to be shown to bind.
5. IDENTITY. Cached and uncached runs produce byte-identical wire output.

## GATE, run each TWICE, paste both runs

```bash
cd v6/sprefa-extract
cargo build --release --features cli --bin extract    # rc=0
cargo test --features cli                             # 0 failed
cd ../sprefa-engine-rs && cargo test -p sprefa-engine-rs   # 0 failed
cd ../.. && python3 v6/tools/soopy-lockstep.py        # PASS: one soopy closure
```

`cargo test --features cli` is THE extract gate. Bare `cargo test` is NOT: the
`extract` bin is behind `required-features`, so bare `cargo test` hands a
nonexistent `CARGO_BIN_EXE_extract` to the CLI tests and fails on a clean tree.

The engine suite is in the gate because this PR changes a signature the engine
calls at three sites.

The `--resolve` path is now parallel, so the timing receipt's baseline is
1.83-2.53 s real at 4.60-4.77 s user with 441 MB peak RSS, measured at
924b8661f. A cold pass must not regress past that.

Timing receipt, before and after, three runs each:

```
FILES=$(git ls-files '*.rs' '*.ts' '*.tsx' '*.js' '*.go' '*.kt' '*.py' '*.html')
/usr/bin/time -l ./v6/sprefa-extract/target/release/extract --resolve $FILES > /dev/null
```

A cold pass must NOT get slower. Report peak RSS too: the cache is memory you
are choosing to hold, so its cost belongs in the PR.

## STYLE LAWS, non-negotiable

- Max 2 consecutive comment lines in new code. Comments state only constraints
  the code cannot show. No change-log narrative, no dates, no arc references.
- No `eprintln!` in `src/**`. `tracing` only.
- No em dashes anywhere, prose or code.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base, critical, mode.
- Descriptive names, never single letters, in every binding.
- NEVER run bare `cargo fmt`. Format only the lines you wrote.
- Every new class or type declares its interface where the package's header
  says; do not invent a new module layout.

## COMMIT AND PR

Trailer:

```
Refs-Issue: @extract-blob-cache-parallel
```

PR body carries: the counted hit/miss table, the eviction receipt, both gate
runs for both crates, the before/after timing and RSS, the capacity rule you
implemented, and the base sha.

Do not merge it yourself. Do not spawn subagents.
