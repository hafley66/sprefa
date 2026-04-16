# 1c — OpCtx + Op + RuntimeConfig extensions and ctor backfill

Extend `OpCtx` with 5 Session-1 fields, add `Op::expansion_mode`,
extend `RuntimeConfig` with 3 knobs, then backfill every existing
`OpCtx { ... }` and `RuntimeConfig { ... }` literal.

## Prereqs
1a (so `NoopStore` and `MutationRequest` exist) and 1b (so field types
resolve cleanly).

## Status
Partial. OpCtx fields added, Op::expansion_mode added, RuntimeConfig
fields added. 5 OpCtx ctors backfilled. 8 RuntimeConfig literal
sites still missing the 3 new fields (the agent was mid-sweep when
stopped).

## Finish: remaining edits

### A. mutations.rs:89 — wrong BufReader import
`tokio::io::BufReader` isn't the right path. Remove the `InteractiveCli`
stdin-reader field entirely for Phase 1 — constructor returns a unit
struct. Phase 2 implements stdin prompt loop.

Change:
```rust
pub struct InteractiveCli { stdin: Mutex<tokio::io::BufReader<...>> }
```
to:
```rust
pub struct InteractiveCli;

impl InteractiveCli {
    pub fn new() -> Self { Self }
}
```
Remove the now-unused `Mutex` / `BufRead` imports.

### B. RuntimeConfig literal backfill
8 sites need 3 fields appended:
```rust
max_passes:           8,
max_claims_per_pass:  10_000,
max_cursors_per_root: 1_000_000,
```

Sites (from `cargo build --tests` error output):
- `v2/src/_5_op.rs:964`
- `v2/src/_14_scan_loop.rs:186`
- `v2/src/analysis.rs:331`
- `v2/src/analysis.rs:508`
- `v2/src/analysis.rs:758`
- `v2/src/analysis.rs:1439`
- `v2/src/ops/_0_rule.rs:499`
- `v2/src/ops/_3_fs.rs:342`

(Run `cargo build --tests 2>&1 | grep "missing fields" -B2` to confirm
the list if line numbers drift.)

## Verify
```
cd v2 && cargo build --tests 2>&1 | tail -30
```
Expect zero errors. Unused-import warnings in `mutations.rs` acceptable
until Phase 2.

Also:
```
cd v2 && cargo test --lib -p v2 2>&1 | tail -10
```
Existing tests should still pass (OpCtx test builders use the
`NoopStore` stub pattern).

## Reference: OpCtx field additions (already landed)

For documentation when reading the diff:

```rust
pub store:        Arc<dyn crate::store::Store>,
pub mutations:    tokio::sync::mpsc::Sender<crate::mutations::MutationRequest>,
pub cancel:       tokio_util::sync::CancellationToken,
pub expr_name:    Option<Arc<str>>,
pub current_site: Arc<ParseSite>,
```

Op trait default:
```rust
fn expansion_mode(&self) -> ExpansionMode { ExpansionMode::Exhaustive }
```

ExpansionMode:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionMode { Exhaustive, Demand }
```

Stub-value pattern used in 5 OpCtx ctors:
```rust
store:        Arc::new(crate::store::NoopStore::new()) as Arc<dyn crate::store::Store>,
mutations:    tokio::sync::mpsc::channel::<crate::mutations::MutationRequest>(32).0,
cancel:       tokio_util::sync::CancellationToken::new(),
expr_name:    None,
current_site: Arc::new(crate::_0_types::ParseSite {
    file:       Arc::from(std::path::Path::new("")),
    path:       Arc::from(Vec::<crate::_0_types::ParseSeg>::new().into_boxed_slice()),
    byte_range: 0..0,
}),
```

## Exit state
- `cargo build --tests` green
- All 11 OpCtx fields present at every construction site
- All RuntimeConfig literals carry the 3 new fields
- Ready for 1d (commit)
