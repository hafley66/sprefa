use crate::{SourceId, Span};

#[derive(Clone, Debug)]
pub struct Source {
    pub id: SourceId,
    pub text: String,
}

impl Source {
    pub fn span(&self, start: usize, end: usize) -> Span {
        Span::new(self.id, start, end)
    }
}
