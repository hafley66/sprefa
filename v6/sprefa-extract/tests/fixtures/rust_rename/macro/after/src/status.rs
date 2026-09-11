use crate::{Inventory, _1b_ground};

pub fn total(inventory: &Inventory) -> u8 {
    inventory.ground + _1b_ground::decide()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decides() {
        assert_eq!(_1b_ground::decide(), 128);
    }
}
