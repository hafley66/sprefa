pub struct Editor {
    pub text: String,
}

impl Editor {
    pub fn build() -> Option<Editor> {
        Some(Editor { text: String::new() })
    }

    pub fn finish(&self) -> usize {
        self.text.len()
    }

    pub fn replace(&self) -> usize {
        self.text.len() + 1
    }
}
