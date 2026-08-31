//! Same-named defs in another file. Without them a corpus-wide name match binds
//! every site by name alone and no test can tell the checker apart from it.

pub struct Panel;

impl Panel {
    pub fn replace(&self) -> usize {
        0
    }

    pub fn render(&self) -> usize {
        0
    }

    pub fn label(&self) -> usize {
        0
    }
}

pub fn helper() -> usize {
    0
}
