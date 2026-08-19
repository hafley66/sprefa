/**
 * The TS door's boot check. `ProgramCompiler.compile` runs every loaded module
 * through `IrVersionCheck.check`, so a module emitted at another ir_version is
 * refused by name instead of being interpreted field by field.
 *
 * SABOTAGE RECEIPT: returning `program` unconditionally from
 * `IrVersionCheck.check` reds both refusal cases below and leaves the third
 * green.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { IrVersionCheck, RUNTIME_IR_VERSION } from "../runtime/irVersion.ts";
import type { IServedProgram } from "../runtime/types.ts";

function program(ir_version: number | undefined): IServedProgram {
  return { name: "probe", ir_version } as unknown as IServedProgram;
}

test("irVersion: a module emitted at another ir_version is refused by name", () => {
  assert.throws(
    () => IrVersionCheck.check(program(RUNTIME_IR_VERSION + 1)),
    new RegExp(
      `^Error: ir_version_mismatch: program probe was emitted at ir_version ${RUNTIME_IR_VERSION + 1} ` +
        `and this runtime interprets ${RUNTIME_IR_VERSION}$`,
    ),
  );
});

test("irVersion: a module stamping no ir_version is refused too", () => {
  assert.throws(() => IrVersionCheck.check(program(undefined)), /emitted at ir_version none/);
});

test("irVersion: the runtime's own ir_version passes the module through", () => {
  const loaded = program(RUNTIME_IR_VERSION);
  assert.equal(IrVersionCheck.check(loaded), loaded);
});
