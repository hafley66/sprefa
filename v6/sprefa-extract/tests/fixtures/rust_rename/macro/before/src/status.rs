use crate::{Inventory, ground};

pub fn total(inventory: &Inventory) -> u8 {
    inventory.ground + ground::decide()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decides() {
        assert_eq!(ground::decide(), 128);
    }
}
