trait Shape {
    fn area(&self) -> u32 {
        1
    }
}

struct Sq;

impl Shape for Sq {
    fn area(&self) -> u32 {
        4
    }
}

fn f(s: &dyn Shape) {
    s.area();
}
