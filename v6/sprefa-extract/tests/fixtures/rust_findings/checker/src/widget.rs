pub struct Widget;

impl Widget {
    pub fn render(&self) -> u32 {
        1
    }
}

pub fn make() -> Widget {
    Widget
}
