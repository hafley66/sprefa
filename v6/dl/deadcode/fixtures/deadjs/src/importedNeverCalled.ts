// knip: SILENT. index.ts imports shelvedOne, and an import is a use to the
// module graph. rail: MUST FIRE. Nothing ever CALLS anything here, which is
// the distinction a call graph draws and an import graph cannot. The mirror of
// dead_pub.rs in the Rust fixture, with the two tools swapped.
export function shelvedOne(): number { return 4; }
function shelvedTwo(): number { return 5; }
function shelvedThree(): number { return 6; }
function shelvedFour(): number { return 7; }
function shelvedFive(): number { return 8; }
