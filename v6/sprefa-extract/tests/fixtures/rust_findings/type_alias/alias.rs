pub struct Inner {
    pub value: u32,
}

pub struct Wrapped<T> {
    pub item: T,
}

pub type Handle = Inner;

pub type Boxed = Wrapped<Inner>;
