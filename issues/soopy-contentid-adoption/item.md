---
created: 2026-08-16
updated: 2026-08-16
type: improvement
reporter: chris
status: done
priority: normal
epic: soopy-full-wiring
closed: 2026-08-16
---

# Adopt soopy ContentId in place of BlobHash

## Description

BlobHash = blake3 cut to 16 bytes (types.rs:55), 82 refs, incomparable with ContentId::GitBlob/Blake3, forces extract's own blake3 dep (Cargo.toml:89). Adopt soopy::ContentId so a digest from repo_files_at and a digest from extract are the same value. Mechanical but wide; sql-relational-design + sqlite-costs are mandatory reads for any stored-key width change. Candidate 4.

## Resolution

### 2026-08-16T23:55:54Z · @issuectl

Merged as PR #322 (src 331a2fa21 + tests eeb60de49): BlobHash deleted, full-width ContentId, 129 extract tests green, engine compiles. blake3 dep remains in extract Cargo.toml pending a public soopy constructor for the Blake3 arm.
