use crate::alpha::Alpha;
use crate::shape::Shape;

pub fn alpha_user() -> Alpha {
    Alpha::First(3)
}

pub fn collide_user() -> Shape {
    let _side = square(2);
    Shape::Square(2)
}
