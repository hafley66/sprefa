pub enum Shape {
    Round(u32),
    Square(u32),
}

impl Shape {
    pub fn make(radius: u32) -> Shape {
        Self::Round(radius)
    }
}
