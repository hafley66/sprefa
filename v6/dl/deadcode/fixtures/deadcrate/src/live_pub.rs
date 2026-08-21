// rustc: silent. rail: silent (lib.rs calls exported_one).
pub fn exported_one() -> usize { exported_two() }
pub fn exported_two() -> usize { shared_three() }
pub fn shared_three() -> usize { shared_four() }
pub fn shared_four() -> usize { shared_five() }
pub fn shared_five() -> usize { 1 }
