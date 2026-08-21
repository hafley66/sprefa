// knip: FIRES, "unused file" -- no module imports it. rail: MUST FIRE, no call
// site anywhere names its definitions. The case both tools can see.
export function orphanOne(): number { return orphanTwo(); }
function orphanTwo(): number { return orphanThree(); }
function orphanThree(): number { return orphanFour(); }
function orphanFour(): number { return orphanFive(); }
function orphanFive(): number { return 3; }
