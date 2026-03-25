# Architecture Decisions

## Crate split: index vs cache

The extraction pipeline has no business touching a database. Split:

```
crates/index   -- pure Rust, no sqlx, no async
                  git2 tree walk + blob reading
                  rayon parallel extract
                  in-memory resolver
                  output: Vec<FileRefs>

crates/cache   -- SQLite progressive cache
                  takes Vec<FileRefs>, writes deltas
                  pre-scan context load (known files, known strings)
                  one transaction per repo flush
```

Testing the extraction logic requires no DB setup -- `index::scan_repo()` returns
plain data. The DB is an optional consumer.

## File enumeration: libgit2, not shell

Replace `git ls-files` subprocess + mmap with libgit2 tree walk + blob content.

```rust
let repo = git2::Repository::open(path)?; // bare or normal, transparent
let tree = repo.revparse_single(branch)?.peel_to_tree()?;
// walk tree, read blobs as &[u8], pass to extractors
```

Works on bare clones (no working tree on disk) and normal clones identically.
Eliminates the subprocess, handles any branch without checkout.

## Git tool integration

Sprefa's sister tool manages repo syncing, PR tracking, and branch head polling.
It stores bare clones in a staging directory and maintains its own SQLite DB
with PR state and branch hashes.

Sprefa reads from that tool instead of maintaining its own repo config:
- repo paths and names from git tool TOML config
- changed files / branch heads from git tool SQLite DB
- no `sprefa add`, no `[[repos]]` in sprefa.toml needed for managed repos

Integration points:
- git tool detects branch update -> calls `sprefa scan --repo X --branch Y`
  or hits daemon `POST /scan` with changed file list
- sprefa compares branch HEAD hash against last scanned hash to skip unchanged files

## Flush: no DB in hot path

Scan phases:
1. pre_scan(db) -> ScanContext { known_files, known_strings, repo_ids }
2. extract phase: pure Rust, rayon parallel, checks ScanContext to skip unchanged
3. flush phase: one transaction per repo, bulk inserts only for deltas

ScanContext loaded per-repo (not globally) to bound memory. `known_strings` is
the largest map -- bounded by unique string count in one repo, not total index.

## Bulk insert strategy

All DB writes batched into one transaction per repo. Chunked multi-row VALUES
inserts to stay under SQLite's 32766 bound parameter limit:

- strings: 2000 rows/chunk (3 params each)
- files:   2000 rows/chunk (5 params each)
- refs:    1000 rows/chunk (8 params each)

String dedup done entirely in Rust (HashSet) before any DB contact.
File id map loaded with one `SELECT path, id FROM files WHERE repo_id = ?` after
bulk file insert. String ids loaded with chunked IN queries after bulk string insert.

Result: ~250 statements per repo scan regardless of repo size, vs O(N*M) before.
