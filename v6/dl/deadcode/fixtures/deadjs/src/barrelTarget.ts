// knip: SILENT, the module graph reaches it through barrel.ts. rail dead
// bucket: MUST FIRE, no call site anywhere names these definitions. rail
// unreachable bucket: MUST STAY SILENT, because `export * from` is an import
// edge and the crawl follows import edges. Name matching alone cannot reach
// it: a star re-export writes no callee.
export function barreledOne(): number { return 11; }
export function barreledTwo(): number { return 12; }
export function barreledThree(): number { return 13; }
export function barreledFour(): number { return 14; }
export function barreledFive(): number { return 15; }
