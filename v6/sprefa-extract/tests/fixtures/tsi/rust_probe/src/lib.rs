// Every construct here is one the parse alone cannot answer: an associated
// type, a lifetime, a borrow against an owned box, a type that names itself.

pub trait Mapper<T> {
    type Output;

    fn map(&self, element: T) -> Self::Output;
}

pub struct User<T> {
    pub id: T,
    pub name: Option<String>,
}

impl<T> Mapper<T> for User<T> {
    type Output = Vec<T>;

    fn map(&self, element: T) -> Self::Output {
        vec![element]
    }
}

pub struct View<'a> {
    pub text: &'a str,
    pub owned: Box<User<u32>>,
    pub shared: std::rc::Rc<View<'a>>,
}

pub enum Shape {
    Circle(f64),
    Square { side: f64 },
}
