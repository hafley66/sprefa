// knip: SILENT, the entry re-exports one of these under another name. rail dead
// bucket: MUST FIRE, nothing calls these names. rail unreachable bucket: MUST
// STAY SILENT. `export { renamedOne as publicRenamed } from` publishes a name
// that appears in NO definition anywhere, so a name-matching crawl following
// the published name lands on nothing.
export function renamedOne(): number { return 41; }
export function renamedTwo(): number { return 42; }
export function renamedThree(): number { return 43; }
export function renamedFour(): number { return 44; }
export function renamedFive(): number { return 45; }
