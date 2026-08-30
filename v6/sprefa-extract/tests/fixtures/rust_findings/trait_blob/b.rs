trait Shape {
    fn area(&self) -> u32 {
        2
    }
}

struct Cc;

impl Shape for Cc {}

fn g(s: &dyn Shape) {
    s.area();
}
