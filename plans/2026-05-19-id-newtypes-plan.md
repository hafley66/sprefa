# v4 id newtypes plan — 2026-05-19

Target findings: items 15 (bare aliases) and 16 (hash truncation) of `plans/2026-05-19-v4-worst-audit.md`. Plus the five `String` ids on `Subscribe<R>` / `SupportRows<R>` / `ActiveChild<R>` in `v3/crates/effect_runtime/src/v2/runtime_graph.rs`.

## Goals

- Cross-family assignment becomes a compile error.
- Content-hash collisions either cannot happen at the chosen width or are detected and surfaced on insert.
- On-disk sqlite layout stays decodable. No destructive rewrite of existing `.sprf/*.db`.
- The `v3/effect_runtime` ↔ `v4` boundary keeps the existing `NodeId` trait seam; we widen what implements it, not the seam itself.

## Design — type signatures first

### 1. Single id newtype macro in `v4/src/lib.rs`

Replace `v4/src/lib.rs:99-104` with one declarative macro and six invocations.

```rust
macro_rules! content_id_u64 {
    ($Name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        #[repr(transparent)]
        pub struct $Name(pub u64);
        impl $Name {
            pub const SYNTHETIC: Self = Self(0);
            pub fn raw(self) -> u64 { self.0 }
        }
    };
}
content_id_u64!(FileId);
content_id_u64!(RefId);   // see §3 — kept for AST stability
content_id_u64!(PathId);
content_id_u64!(BlobId);

content_id_u32!(RepoId);
content_id_u32!(RevId);
```

`#[repr(transparent)]` is the contract that lets the sqlite codec keep its current `to_string()` / `parse()` integer round-trip without an alignment shift.

Lifetimes: all `Copy + 'static`. They never carry references.

### 2. `Ref` and `WhereBytesId` no longer wrap `RefId = u64`

Today `v4/src/lib.rs:160-185` defines `Ref(pub RefId)` where `RefId = u64`. Reshape:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Ref(pub RefHash);  // RefHash is a tuple-struct, see §3
```

`Ref::of(c: Coord) -> Ref` and `WhereBytesId::of(w: WhereBytes) -> WhereBytesId` build a `RefHash` (full 16 bytes — see §3), not a truncated `u64`.

### 3. Hash widening

Three options, ranked:

A. **u128, full 128 bits.** Birthday horizon ≈ 2^64. Cost: sqlite cells go from 16-hex to 32-hex (or 16 bytes blob). Recommended default for `RefHash`, `FileId`, `PathId`, `BlobId`.

B. **Full 32-byte blake3.** Overkill for FK ids. Keep as the existing separate `content_hash` blob.

C. **Indirect via intern table.** Eliminates collision class but inverts the design — sprf is explicitly "no central allocator". Out of scope.

**Decision: option A for FileId / PathId / BlobId / RefHash (u128). Option A for RepoId / RevId widened from u32 to u64** (`v4/src/store.rs:520, 568`). RepoId at u32 has a ~2^16 horizon exposed today.

For each id, add an **insert-time collision detector**. Today `seen_repos: DashSet<u64>` only tracks first-seen. Extend to `DashMap<Id, ContentFingerprint>` where fingerprint is the 32-byte blake3 of `(slug, remote)` / `(repo, oid)` / `(content,)`. On second insert with matching id, compare fingerprints. Mismatch surfaces as a structured error.

### 4. The `Ref::SYNTHETIC` zero-prefix routing bug

`v4/src/lib.rs:170`. Real coords whose blake3 prefix happens to be zero return a non-synthetic `Ref(0)` indistinguishable from sentinel. Fix in §2's wider hash, plus `debug_assert!(hash != 0u128)` and a deterministic rehash on hit (prepend a domain byte, re-hash). Domain-tagged hashing via `blake3::keyed_hash` with a per-family key prevents zero prefix and cross-family hash equality.

### 5. `String` ids on `Subscribe<R>` / `SupportRows<R>` / `ActiveChild<R>`

`v3/crates/effect_runtime/src/v2/runtime_graph.rs:306-336, 364-407, 409-461` carry eight ids all typed `String`.

Sprf v4 calls through `SprfSubscribe::new` (`v4/src/runtime_graph.rs:353`) using typed `OwnerNode`, `SourceNode`. The bare `Subscribe<R>` is used inside `effect_runtime` tests and `RuntimePut` apply paths.

Two-step fix:

Step A — parameterise on existing `NodeId` trait (`v3/.../runtime_graph.rs:59`). Replace `Subscribe<R>` with `Subscribe<R, I: NodeId = String>`. Default keeps internal tests compiling. v4 already projects through typed handles.

Step B — fingerprint the fields by role. Introduce two phantom-typed wrappers v3-side:

```rust
pub struct NodeUriId<I: NodeId>(pub I);
pub struct LabelId<I: NodeId>(pub I);
```

Subscribe becomes `Subscribe<R, I> { owner_id: NodeUriId<I>, label_id: LabelId<I>, source_id: NodeUriId<I>, edge_id: NodeUriId<I>, subscribe_kind_id: LabelId<I>, ... }`. Storage stays `String` columns; `as_id_str()` projects through.

## Storage layout / migration

The on-disk encoding is a string per cell (`v4/src/store.rs:217-220`). u64 ids today serialise as decimal strings; widening to u128 requires:

- `Cursor::set(col, value: &str)` is untyped, so it does not care. Decode-side (`row.get("id").parse::<u64>()`) is the boundary that needs to learn u128. Audit every `parse::<u64>()` site.
- No schema change required for runtime_node / runtime_edge / runtime_value / runtime_continuation / runtime_dirty.
- `_files`, `_repos`, `_revs`, `_paths`, `_refs`, `_where_bytes` PKs are decimal strings. u128 written as decimal coexists with u64-old rows.
- `content_hash` column on `_files` is already 64-hex. Untouched.

**Forward migration (no destructive rewrite):**

1. Land newtypes with widened hash behind a build-time `#[cfg(feature = "narrow_ids")]` defaulting off. Existing `.sprf` dbs continue to read.
2. New writes use u128. Existing u64-prefix rows remain queryable. Re-interning the same `(slug, remote)` post-upgrade produces a new u128 id that does not collide with the old u64 row.
3. A one-shot `sprf_migrate_ids` admin op (optional) can rewrite FKs by re-deriving u128 ids inside a transaction.

## Shim vs hard cut

**Shim (newtype + `From<u64>`):** any current `RepoId = u32` literal `0` continues to work via `.into()`. Cheap, lets v2_ops compile unchanged. Cost: the `From` impl is exactly the hole the audit closes.

**Hard cut (no implicit conversions):** every literal `0` becomes `RepoId::SYNTHETIC`; every `as RepoId` cast becomes a constructor visible only inside `store.rs`. ~80-120 mechanical touches.

**Recommendation: hard cut.** A `From` shim re-opens the bug. Churn is one-time; silent-miswire risk is permanent.

## File-by-file change list

`v4/src/lib.rs:99-104` — newtypes (§1).
`v4/src/lib.rs:160-233` — `Ref`, `WhereBytesId` wider hash (§2-§4).
`v4/src/lib.rs:243` — `CursorValue::Blob(BlobId)` already typed.
`v4/src/lib.rs:255-269` — `StringId` widen to u128.
`v4/src/lib.rs:273-279` — `file_id_of` returns `FileId`, hashes to u128.

`v4/src/store.rs:18` — re-export updated ids.
`v4/src/store.rs:93-103` — LRU key types switch to newtypes.
`v4/src/store.rs:233, 256, 266, 278-282` — synthetic-row writes use `RepoId::SYNTHETIC`.
`v4/src/store.rs:336-353` — `intern_file` u128 + collision check.
`v4/src/store.rs:355-375` — `lookup_file` `parse::<u128>`.
`v4/src/store.rs:511-534` — `intern_repo` u64 widening.
`v4/src/store.rs:559-582` — `intern_rev` u64 widening.
`v4/src/store.rs:609-660` — parse boundaries widen.
`v4/src/store.rs:100-103` — `seen_*: DashMap<Id, [u8; 32]>`.

`v4/src/source.rs:6, 20, 128-129` — re-import newtypes.

`v4/src/v2_ops.rs:933-935, 1165, 1848, 2655, 3262, 3357` — `0 as FileId` → `FileId::SYNTHETIC`.

`v4/src/compile/binding_graph.rs:517-1306`, `compile/fuser.rs:155-540`, `compile/lower/liftable.rs:35-37`, `compile/lower/ops.rs:1609-1639` — TypeLattice bridge.

`v3/crates/effect_runtime/src/v2/runtime_graph.rs:204-216, 282-292, 306-336, 364-407, 409-461` — `Subscribe / SupportRows / ActiveChild / RuntimeEdge / RuntimeNode / RuntimeValue` parameterised on `I: NodeId = String`; role wrappers (§5).

`v3/crates/effect_runtime/src/v2/tests.rs:271, 279, 331, 399` — `NodeUriId(...)`-shaped construction.

`v4/src/runtime_graph.rs:18-94` — thread `NodeUriId<StringId>` across `Subscribe<R, I>` boundary.

## Testing

- **Compile gate** primary. If `cargo build -p v4` passes after hard cut, cross-family assignment is provably gone.
- `v4/tests/id_newtype_compile_fail.rs` (trybuild or doc-test `compile_fail`). Each family asserts `let _: FileId = RepoId::SYNTHETIC;` fails to compile.
- `v4/tests/id_collision_detect.rs` — synthesise two `(slug, remote)` pairs that collide under chosen truncation; assert second insert surfaces structured error.
- Existing 58 tests unchanged.
- u64→u128 round-trip test: re-open a pre-change `.sprf` db, query, observe old rows decode and new writes use wider ids.

## Risks across v3 ↔ v4 boundary

1. **`NodeId` trait signature** is the structural seam: `as_id_str(&self) -> Cow<'_, str>` and `from_id_str(&str) -> Self`. String-shaped; widening `StringId` to u128 only changes the decimal-string width.
2. **`Subscribe<R>::owner_id: String`** change is generic-defaulted; only `v4`'s `SprfSubscribe` adapter changes. Grep across `v3` shows zero call sites outside `tests.rs`.
3. **`Generation(pub u64)` publicly constructible** (audit item 13). Out of scope here; consider folding into same newtype-discipline PR.
4. **`row.get("id").parse::<u64>().unwrap_or(0)` sites** silently widen if missed. Mitigation: route every parse through `FileId::from_decimal_str(s) -> Result<FileId, IdDecodeError>`.

## Estimate

- v4: ~25-35 files touched.
- v3/effect_runtime: 2-3 files.
- New test files: 2.
- Net lines: +250 / -120 in v4, +60 / -30 in v3.
- One PR if reviewable in 90 min; otherwise split into 3 (hash widening + collision detector; newtypes hard cut; Subscribe<R,I>).
