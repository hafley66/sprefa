use crate::macros::{mint_helpers, mint_single};

pub struct Widget;

mint_helpers!(Widget);
mint_single!(Widget);

pub fn helper_one() -> u32 {
    1
}
pub fn helper_two() -> u32 {
    2
}
