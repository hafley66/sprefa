/**
 * diff.test.ts — unit coverage for the multiset diff every gen/*.ts tick
 * uses (runtime/diff.ts): ordinary set behavior for Set/level rows, and the
 * duplicate-preserving Log-append case (two occurrences of the same row
 * value in one tick both count).
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { multisetDiff } from "../runtime/diff.ts";

test("multisetDiff: plain set-style add and del", () => {
  const before = [["a", "1"], ["b", "2"]];
  const after = [["b", "2"], ["c", "3"]];
  const result = multisetDiff(before, after);
  assert.deepEqual(result.add, [["c", "3"]]);
  assert.deepEqual(result.del, [["a", "1"]]);
});

test("multisetDiff: unchanged rows produce no delta", () => {
  const rows = [["x", "1"]];
  const result = multisetDiff(rows, rows);
  assert.deepEqual(result, { add: [], del: [] });
});

test("multisetDiff: duplicate row values are counted, not deduped (Log append)", () => {
  const before = [["alpha"]];
  const after = [["alpha"], ["alpha"], ["alpha"]];
  const result = multisetDiff(before, after);
  assert.deepEqual(result.add, [["alpha"], ["alpha"]]);
  assert.deepEqual(result.del, []);
});
