// rustc: silent. rail: silent (lib.rs calls helper_one).
pub fn helper_one() -> usize { helper_two() }
pub fn helper_two() -> usize { helper_three() }
pub fn helper_three() -> usize { helper_four() }
pub fn helper_four() -> usize { helper_five() }
pub fn helper_five() -> usize { 2 }
