pub trait Render {
    fn render(&self) -> u32;
}

pub trait Shade {
    fn shade(&self) -> u32;
}
