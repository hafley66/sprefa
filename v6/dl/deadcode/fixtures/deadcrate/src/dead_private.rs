// rustc: FIRES, `function is never used`, once per item. rail: MUST FIRE.
// Both tools can see this one, so it pins that the rail is not merely finding
// a different thing than the established lint.
fn buried_one() -> usize { buried_two() }
fn buried_two() -> usize { buried_three() }
fn buried_three() -> usize { buried_four() }
fn buried_four() -> usize { buried_five() }
fn buried_five() -> usize { 4 }
