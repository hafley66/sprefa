// C1 negative: a value that does not fill the spliced width.
// EXPECTED: refused, and the message names the spliced arity.

type ARow = [id: number, name: string];
type BRow = [...ARow, extra: number];

const bShort: BRow = [1, "n"];

export { bShort };
