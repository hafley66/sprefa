/**
 * 4_ingest.ts — extract subprocess stdout -> spine EDB rels -> per-file re-ingest DIFF.
 *
 * Contract (plan M3, tasks.d.ts): `extractFile(path)` spawns DL_EXTRACT_BIN and yields
 * ExtractRecords; `toFactLines(recs, path)` is the pure F2 mapping (record=node|edge|
 * sig|site|const -> spine rel rows, plus computed span_line(path, start, line, col)
 * rows from file bytes); `ingestFile` diffs the new per-path row set against the current
 * tables and rides ONE rt.commit (retraction is the diff, no diag-specific code).
 * extract is CONSUME-ONLY: never touch its crate or worktree.
 *
 * Owned by package M3 (ingest). Placeholder until that package lands.
 */
export {};
