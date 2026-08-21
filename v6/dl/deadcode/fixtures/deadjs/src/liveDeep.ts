// knip: silent. rail: silent. Reached from the entry through one hop.
export function helperOne(): number { return helperTwo(); }
function helperTwo(): number { return helperThree(); }
function helperThree(): number { return helperFour(); }
function helperFour(): number { return helperFive(); }
function helperFive(): number { return 2; }
