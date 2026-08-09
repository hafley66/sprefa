# Fixture corpus convention

`tests/bench_grid.rs` builds one grid row per `Registry::discover().all()`
harness, sourced from `tests/fixtures/<harness.id()>/sessions.json`. The bench
never names a harness id in code; it only reads whatever manifest sits at that
path. A harness with no manifest gets a `no corpus` row, never a skip.

## `sessions.json`

A JSON array of session entries, each field mapping straight onto
`boop::harness::SessionRef`:

```json
[
  {
    "session_id": "bench-example-0001",
    "nickname": "bench-example-0001",
    "path": "bench/bench-example-0001.jsonl",
    "cwd": "/bench/example",
    "git_branch": "bench",
    "parent": null
  }
]
```

- `path` is relative to the harness's fixture dir (`tests/fixtures/<id>/`),
  not to `sessions.json` itself.
- The underlying file(s) `path` points at are opaque to the bench: a JSONL
  transcript for a file-backed harness, a SQLite store for a DB-backed one,
  whatever `Harness::read_from` / `Harness::ingest` for that adapter expects.
  Two manifest entries may point at the same file (e.g. two sessions inside
  one SQLite store).
- Size the corpus for the whole `cargo test` run to stay under 10s: hundreds
  to low thousands of events per harness, never a real multi-GB transcript
  clone.

## Adding a new adapter's corpus

1. Pick a small, representative sample of that harness's real record shape.
2. Write it under `tests/fixtures/<id>/bench/` (or reuse an existing
   `tests/fixtures/<id>/` fixture file already on disk).
3. Add `tests/fixtures/<id>/sessions.json` pointing at it.
4. Nothing else: `tests/bench_grid.rs` picks the new corpus up on the next
   `cargo test` run once the adapter is registered in `Registry::discover()`.
