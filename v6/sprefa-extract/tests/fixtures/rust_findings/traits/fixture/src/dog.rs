pub struct Dog;

impl Speak for Dog {
    fn speak(&self) {}
}

pub fn make() -> Dog {
    Dog
}
