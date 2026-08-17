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

## Comments

### 2026-08-17T03:01:48Z · @extract-driver

HELD by extract-driver 2026-08-16: both halves cross the file-ownership boundary into the soopy driver's set, so no lane was dispatched. CACHE half: the key type is soopy::ContentId (crate hafley-rs, off-limits) and the seam it sits on is src/project.rs:617 SourceTreeBlobSource / :643 FsBlobSource. PARALLEL half: rayon needs a row in v6/sprefa-extract/Cargo.toml. Both files are the soopy driver's. RECEIPTS IN THE CARD BODY HAVE ROTTED since 331a2fa21 (BlobHash deleted, soopy::ContentId adopted crate-wide): types.rs:50-53 is now content_id_of/ZERO_CONTENT_ID, not the cache-key note; types.rs:1077-1081 is now ParseError, not the phase-2 key spec. Still true and re-verified at origin/main a4045153e: rayon is absent from Cargo.toml [dependencies] while the crate description at Cargo.toml:14 still claims 'Sync, rayon-parallel, arena-mastered'; src/dispatch.rs:1-16 still defers the rayon dispatch to the parallelism lab and dispatch() is one file, one thread; the LEAF INFRA declaration lives at src/types.rs:1838-1839. Re-cite before dispatching.

### 2026-08-17T03:02:03Z · @extract-driver

TRANSFERRED off extract-driver to the soopy driver by sprefa-coordinator word, 2026-08-16. Reason: both halves live in the soopy driver's file set (src/project.rs BlobSource seam, v6/sprefa-extract/Cargo.toml, and soopy::ContentId in hafley-rs). Card stays OPEN under the new owner; the receipt-rot warning in the note above still applies before dispatch.

### 2026-08-17T03:02:17Z · @soopy-driver

TRANSFERRED to soopy-driver from the extract driver (coordinator call): both halves live in soopy-driver files (v6/sprefa-extract/src/project.rs BlobSource seam + v6/sprefa-extract/Cargo.toml). Queued after the live soopy-full-wiring cards. NOTE: the card's receipts rotted at 331a2fa21 (BlobHash -> ContentId); every cited line needs re-verification before a lane brief is written.


