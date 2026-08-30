use std::mem;

pub fn swap_out(s: &mut String) -> String {
    mem::take(s)
}
