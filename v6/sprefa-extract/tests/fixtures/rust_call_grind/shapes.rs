pub enum Shape {
    Circle { radius: u32 },
    Dot,
}

pub struct Point {
    pub x: u32,
}

pub fn build_circle() -> Shape {
    Shape::Circle { radius: 1 }
}

pub fn build_dot() -> Shape {
    Shape::Dot
}

pub fn origin() -> Point {
    Point { x: 0 }
}
