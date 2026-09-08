# SCIP reliability

Base: `cc46e44357dafd618da2c223118bf9ece57e7b0f`

## Source changes

- Persistent source stages now remove empty directories bottom-up after stale
  staged source files are pruned. The historical root-only staging path and the
  repository-snapshot path use the same empty-directory helper. Both walks stop
  at `target/`; nonempty indexer and unrelated paths remain in place.
- `extract --family scip ROOT --scip-index FILE` loads `FILE` directly without
  consulting `SPREFA_SCIP_INDEX`, cache paths, marker detection, or the indexer
  roster. Missing and invalid explicit files return errors. Clap rejects an
  explicit file combined with `--indexer` or `--scip-build`.
- Existing `ScipFamilyRequest` construction remains source-compatible. New
  library functions `scip_family_from_index` and
  `scip_family_from_index_jsonl` expose the direct-read path; existing
  `scip_family` and named-indexer cache behavior are unchanged.
- Direct construction or exhaustive matching of `FlatFact::ScipIndexRow` must
  account for the two added fields. Serialized consumers receive the new keys.
- `scip_index` records now carry `index_mtime_unix_ms` and `staleness`.
  Timestamp units are milliseconds since the Unix epoch. `staleness` is
  `stale` when a readable indexed document has a later mtime, `uncertain` when
  the index mtime or an indexed document cannot be read, and
  `no_newer_sources` when all indexed documents are readable and none has a
  later mtime. Explicit indexes continue to emit their relation rows when the
  indicator is `stale` or `uncertain`.

## Self-extraction

Initial orientation used the requested existing binary with `DL_TRAIL=0`:

```text
/tmp/sprefa-extract-main-integration-target.LCsjvv/debug/extract \
  --resolve --family call --witness \
  src/scip.rs src/scip_ensure.rs src/project.rs src/bin/extract.rs \
  src/schema.rs src/wire.rs
```

The filtered stream contained 3,241 records. Resolved edges reported these
`resolution_origin` counts: `same_file` 340, `corpus_unique` 137,
`module_plane` 66, `self_type` 40, and `receiver` 21. The stream retained 2,021
`unresolved` rows; sampled evidence included `external`, `inferred`, and
`no_corpus_def` reasons. Exact source spans were read after this filter.

## Validation

All commands used
`CARGO_TARGET_DIR=/tmp/sprefa-extract-scip-reliability-target.IuDQPc`.

- Focused staging regression: 1 passed, 0 failed.
- Focused explicit-index regressions: 3 passed, 0 failed.
- Focused staleness regression: 1 passed, 0 failed.
- Full affected binaries: `scip_freshness` 10 passed, 0 failed;
  `8_scip_families_cli` 21 passed, 0 failed.
- Finished integration gate: `cargo test --features cli --no-fail-fast`, exit
  0.

CI coverage adds deterministic checks for workspace-member directory pruning,
retained live/target/unrelated staged content, direct explicit-index reads with
a sentinel indexer, explicit-over-environment precedence, missing and invalid
explicit files, flag conflicts, all three staleness states, stale-index fact
emission, missing-source fact emission, and schema/help documentation. No CI
coverage was removed.

## Limitations

- Mtime comparison is evidence about filesystem ordering only. Equal or older
  source mtimes do not establish that index contents match source contents.
- Any unreadable indexed document yields `uncertain` unless another readable
  indexed document already provides positive `stale` evidence.
- Staleness evidence is emitted on the `scip` family header. Raw
  `--scip-facts` output retains its existing record set.
