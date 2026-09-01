pub trait Described {
    fn label(&self) -> usize {
        7
    }
}

pub struct Widget {
    pub size: usize,
}

impl Widget {
    pub fn render(&self) -> usize {
        self.size
    }
}

impl Described for Widget {}
