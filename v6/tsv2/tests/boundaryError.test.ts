/** TEST: the message bytes must equal engine-rs `BoundaryError` Display,
 *  seam by seam; a drift on either door breaks this file. */

import assert from "node:assert/strict";
import { test } from "node:test";

import { BoundaryError, list_at_scalar_seam, type ScalarSeam } from "../runtime/boundary.ts";

test("every scalar seam names itself when a list reaches it", () => {
  const seams: readonly (readonly [ScalarSeam, string])[] = [
    ["sql_parameter", "a SQL parameter"],
    ["host_template_argument", "a host template argument"],
    ["arrival_payload", "an arrival payload"],
    ["text_intern", "the text intern plane"],
  ];
  for (const [seam, spelling] of seams) {
    const raised = list_at_scalar_seam(seam);
    assert.ok(
      raised instanceof BoundaryError,
      `${spelling} must answer a typed error, got ${raised.constructor.name}`,
    );
    assert.equal(raised.message, `a list value reached ${spelling}`);
  }
});

test("the typed error is an Error the caller can catch by name", () => {
  const raised = list_at_scalar_seam("sql_parameter");
  assert.ok(raised instanceof Error);
  assert.equal(raised.name, "BoundaryError");
});
