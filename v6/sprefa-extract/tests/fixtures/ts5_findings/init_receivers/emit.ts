// Expected: `printer` binds one hop through `createPrinter`'s declared return
// type, whose name `NodePrinter` is written in printers.ts and imported here
// by nobody; a nested arrow reads the same binding off the lexical chain;
// `writer` has no declared return type and stays a drop.
// Observed at HEAD: every member site drops with reason `inferred`.

import { createPrinter, createWriter } from "./printers";

export function emitNode(): number {
  const printer = createPrinter();
  return printer.writeNode();
}

export function emitText(): number {
  const writer = createWriter();
  return writer.writeText();
}

function run(fn: () => void): void {
  fn();
}

export function emitInClosure(): void {
  const printer = createPrinter();
  run(() => {
    printer.writeNode();
  });
}
