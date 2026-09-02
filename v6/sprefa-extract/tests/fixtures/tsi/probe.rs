// The rust twin of probe.ts. `type Output` is here to stay unemitted: an
// associated type is the semantic tier's row, never the parse's.

pub trait Mapper<T> {
    type Output;

    fn map(&self, element: T) -> T;
}

pub struct User<T> {
    pub id: T,
    pub name: Option<String>,
}

impl<T> Mapper<T> for User<T> {
    type Output = Vec<T>;

    fn map(&self, element: T) -> T {
        element
    }
}

pub type Query = Option<User<u32>>;
