import assert from "node:assert/strict";
import { test } from "node:test";

import { run_file_watch_scale_cell } from "../scripts/5_file-watch-scale.ts";

test("file-watch scale: duplicate events, edits, identical saves, and deletes have exact receipts", async () => {
  const result = await run_file_watch_scale_cell({
    files: 4,
    events_per_file: 3,
    edit_ratio: 0.5,
    delete_ratio: 0.25,
    identical_resave_ratio: 0.5,
  });

  assert.deepEqual(
    {
      files: result.files,
      edit_files: result.edit_files,
      delete_files: result.delete_files,
      identical_resave_files: result.identical_resave_files,
      events: result.events,
      unique_notified_paths: result.unique_notified_paths,
      subscriptions: result.subscriptions,
      watch_roots: result.watch_roots,
      arrivals: result.arrivals,
      extraction_demands: result.extraction_demands,
      ticks: result.ticks,
      watch_batches: result.watch_batches,
      logical_mutations: result.logical_mutations,
      write_amplification: result.write_amplification,
      final_watch_rows: result.exact_final_rows.watch.actual,
      final_seen_rows: result.exact_final_rows.seen.actual,
      duplicate_count: result.duplicate_count,
      stale_count: result.stale_count,
      missing_count: result.missing_count,
      correct: result.correct,
    },
    {
      files: 4,
      edit_files: 2,
      delete_files: 1,
      identical_resave_files: 2,
      events: 27,
      unique_notified_paths: 9,
      subscriptions: 1,
      watch_roots: ["."],
      arrivals: 9,
      extraction_demands: null,
      ticks: 3,
      watch_batches: 3,
      logical_mutations: 7,
      write_amplification: 9 / 7,
      final_watch_rows: 3,
      final_seen_rows: 3,
      duplicate_count: 0,
      stale_count: 0,
      missing_count: 0,
      correct: true,
    },
  );
  assert.equal(result.exact_final_rows.watch.sha256, result.exact_final_rows.watch.expected_sha256);
  assert.equal(result.exact_final_rows.seen.sha256, result.exact_final_rows.seen.expected_sha256);
  assert.ok(result.sql_statements > 0);
  assert.ok(result.wall_ms >= 0);
  assert.ok(result.peak_rss_bytes > 0);
  assert.ok(result.sqlite_bytes > 0);
});
