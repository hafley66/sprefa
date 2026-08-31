use crate::payload::Label;
use crate::payload::Point;

pub struct Wrapped<T> {
    pub item: T,
}

pub enum Shape {
    Empty,
    Dot(Point),
    Named { label: Label },
    Many(Wrapped<Point>),
}
