---
created: 2026-08-16
updated: 2026-08-16
type: improvement
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
---

# leaf infra: content-keyed cache and parallel dispatch

## Description

## Description

The leaf's two declared infra pieces are unbuilt: the content-keyed extraction
cache and parallel dispatch. Every run re-parses every file, single-threaded,
and the crate description already claims otherwise.

## Receipts

| fact | receipt |
|---|---|
| the declaration | `v6/sprefa-extract/src/types.rs:1838-1839` ("LEAF INFRA (pure CPU; still this leaf): parallel dispatch (rayon, arena-per-worker); BlobSource impls + the (BlobHash, lang, mask) content-keyed cache") |
| cache is named pending on the key type itself | `v6/sprefa-extract/src/types.rs:50-53` |
| phase-2 key is specified but unbuilt | `v6/sprefa-extract/src/types.rs:1077-1081` (`(BlobHash, ProjectDigest, FamilyMask)`) |
| dispatch is one file, one thread | `v6/sprefa-extract/src/dispatch.rs:1-16` ("the generic rayon `dispatch` over many `ExtractJob`s + the arena-per-worker budget land in the parallelism lab (epic 4)") |
| rayon is not a dependency | `v6/sprefa-extract/Cargo.toml` `[dependencies]` has no rayon |
| the crate description claims it anyway | `v6/sprefa-extract/Cargo.toml` `description = "... Sync, rayon-parallel, arena-mastered ..."` |
| BlobSource impls DO exist (that half of the note is stale) | `src/project.rs:617` `SourceTreeBlobSource`, `:643` `FsBlobSource` |

## Fix shape

Two separable arcs; file the second separately if they are dispatched apart.

1. CACHE. `(BlobHash, lang, FamilyMask) -> ExtractOutput` for phase 1 and
   `(BlobHash, ProjectDigest, FamilyMask) -> Vec<ProjectEdge<F>>` for phase 2.
   Build-vs-buy applies before any bespoke map: the candidate set is `moka`,
   `quick_cache`, `lru`, and a plain `HashMap` behind the existing sync seam.
   Written candidate-by-candidate analysis first.
2. PARALLEL DISPATCH. rayon over the job list, one arena per worker. Budget-
   capped per the nothing-seizes-the-machine law.

Measure before and after on the scale corpus (`scripts/scale-invariants.sh`);
a cache with no measured hit rate is not landed.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
bash scripts/scale-invariants.sh
```
