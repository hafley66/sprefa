//! Real-moniker shapes for the scip_name descriptor test: a free fn, an inherent
//! method, a trait impl method, a struct field, and an enum variant. Each yields
//! a different RA moniker grammar (`fn().`, `impl#[T]m().`, `impl#[Tr]for#[T]m().`,
//! `T#field.`, `E#V#`), and all must reduce to a bare identifier name.

pub fn free_function() -> u32 {
    0
}

pub struct Widget {
    pub inner_field: u32,
}

impl Widget {
    pub fn inherent_method(&self) -> u32 {
        self.inner_field
    }
}

pub trait Drawable {
    fn draw_shape(&self) -> u32;
}

impl Drawable for Widget {
    fn draw_shape(&self) -> u32 {
        self.inherent_method()
    }
}

pub enum Shape {
    FirstVariant,
    SecondVariant(u32),
}
