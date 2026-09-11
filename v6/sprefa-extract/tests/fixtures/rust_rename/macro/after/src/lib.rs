#[path = "_1b_ground.rs"]
pub mod _1b_ground;

pub mod advance;
pub mod status;

macro_rules! check {
    ($value:expr) => { $value };
}

pub struct Inventory {
    pub ground: u8,
}

pub fn run(inventory: &Inventory) -> u8 {
    let ground = "ground";
    let stepped = check!(_1b_ground::decide());
    let field = check!(inventory.ground);
    let named = check!(ground);
    let _ = ground;
    let _ = named;
    stepped + field
}

pub fn tally(steps: &[u8]) -> u8 {
    let mut total = 0;
    for ground in steps {
        total += check!(*ground);
    }
    total
}
