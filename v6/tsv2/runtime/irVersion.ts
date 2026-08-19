/** The IR shape version this runtime interprets. `emit_ts.pl ir_version/1`
 *  stamps it on every emitted module; `emit_rust.pl` carries the same value. */

import type { IIrVersionCheck, IServedProgram } from "./types.ts";

export const RUNTIME_IR_VERSION = 1;

export const IrVersionCheck: IIrVersionCheck = {
  runtime_ir_version: RUNTIME_IR_VERSION,

  check(program: IServedProgram): IServedProgram {
    if (program.ir_version === RUNTIME_IR_VERSION) return program;
    const found = program.ir_version === undefined ? "none" : String(program.ir_version);
    throw new Error(
      `ir_version_mismatch: program ${program.name} was emitted at ir_version ${found} ` +
        `and this runtime interprets ${RUNTIME_IR_VERSION}`,
    );
  },
};
