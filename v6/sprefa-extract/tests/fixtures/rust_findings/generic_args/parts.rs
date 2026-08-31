pub struct Payload {
    pub value: u32,
}

pub struct Other {
    pub value: u32,
}

pub trait Carrier<T> {
    fn carry(&self) -> T;
}

pub trait Plain {}
