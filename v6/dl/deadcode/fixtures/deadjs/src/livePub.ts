// knip: silent. rail: silent. index.ts imports and calls exportedOne. Only the
// entry symbol is exported; the helpers stay module-private so knip has no
// unused export to report and the comparison stays file-level.
export function exportedOne(): number { return exportedTwo(); }
function exportedTwo(): number { return sharedThree(); }
function sharedThree(): number { return sharedFour(); }
function sharedFour(): number { return sharedFive(); }
function sharedFive(): number { return 1; }
