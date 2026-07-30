import assert from "node:assert/strict";
import { test } from "node:test";

import { runFileWatchScaleCell } from "../scripts/5_file-watch-scale.ts";

test("file-watch scale: duplicate events, edits, identical saves, and deletes have exact receipts", async () => {
  const result = await runFileWatchScaleCell({
    files: 4,
    eventsPerFile: 3,
    editRatio: 0.5,
    deleteRatio: 0.25,
    identicalResaveRatio: 0.5,
  });

  assert.deepEqual(
    {
      files: result.files,
      editFiles: result.editFiles,
      deleteFiles: result.deleteFiles,
      identicalResaveFiles: result.identicalResaveFiles,
      events: result.events,
      uniqueNotifiedPaths: result.uniqueNotifiedPaths,
      subscriptions: result.subscriptions,
      watchRoots: result.watchRoots,
      arrivals: result.arrivals,
      extractionDemands: result.extractionDemands,
      ticks: result.ticks,
      watchBatches: result.watchBatches,
      logicalMutations: result.logicalMutations,
      writeAmplification: result.writeAmplification,
      finalWatchRows: result.exactFinalRows.watch.actual,
      finalSeenRows: result.exactFinalRows.seen.actual,
      duplicateCount: result.duplicateCount,
      staleCount: result.staleCount,
      missingCount: result.missingCount,
      correct: result.correct,
    },
    {
      files: 4,
      editFiles: 2,
      deleteFiles: 1,
      identicalResaveFiles: 2,
      events: 27,
      uniqueNotifiedPaths: 9,
      subscriptions: 1,
      watchRoots: ["."],
      arrivals: 9,
      extractionDemands: null,
      ticks: 3,
      watchBatches: 3,
      logicalMutations: 7,
      writeAmplification: 9 / 7,
      finalWatchRows: 3,
      finalSeenRows: 3,
      duplicateCount: 0,
      staleCount: 0,
      missingCount: 0,
      correct: true,
    },
  );
  assert.equal(result.exactFinalRows.watch.sha256, result.exactFinalRows.watch.expectedSha256);
  assert.equal(result.exactFinalRows.seen.sha256, result.exactFinalRows.seen.expectedSha256);
  assert.ok(result.sqlStatements > 0);
  assert.ok(result.wallMs >= 0);
  assert.ok(result.peakRssBytes > 0);
  assert.ok(result.sqliteBytes > 0);
});
