pub trait Talk {
    fn chat(&self);
    fn level() -> u32 {
        5
    }
}

pub trait Speak {
    fn speak(&self);
    fn greet(&self) {}
    fn helper() -> u32 {
        7
    }
}
