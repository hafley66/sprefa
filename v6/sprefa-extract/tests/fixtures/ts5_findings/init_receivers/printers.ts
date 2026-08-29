// The initializer callee and the type it returns live HERE; `emit.ts` imports
// the callee only, never the type, which is the corpus shape
// (`const printer = createPrinter(); printer.writeNode(...)`).

export class NodePrinter {
  writeNode(): number {
    return 1;
  }
}

export class TextWriter {
  writeText(): number {
    return 2;
  }
}

export function createPrinter(): NodePrinter {
  return new NodePrinter();
}

export function createWriter() {
  return new TextWriter();
}
