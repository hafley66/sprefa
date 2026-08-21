// knip: SILENT, tsconfig.json maps `@fixture/*` onto `src/*`. rail dead bucket:
// MUST FIRE, nothing calls these names. rail unreachable bucket: MUST STAY
// SILENT. The specifier is BARE, so no directory-relative arithmetic reaches
// it and the resolver has to fall through to its last-segment arm.
export function aliasedOne(): number { return 51; }
export function aliasedTwo(): number { return 52; }
export function aliasedThree(): number { return 53; }
export function aliasedFour(): number { return 54; }
export function aliasedFive(): number { return 55; }
