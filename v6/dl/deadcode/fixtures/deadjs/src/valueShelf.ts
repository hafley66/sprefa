// knip: SILENT, the entry imports the default export. rail dead bucket: MUST
// FIRE, the entry stores the binding in an object and never calls it. rail
// unreachable bucket: MUST STAY SILENT. A default import binds a LOCAL name
// the exporting file never writes, so a call on that local name would name
// nothing here either; only the import edge reaches this file.
export default function valueShelfOne(): number { return 31; }
export function valueShelfTwo(): number { return 32; }
export function valueShelfThree(): number { return 33; }
export function valueShelfFour(): number { return 34; }
export function valueShelfFive(): number { return 35; }
