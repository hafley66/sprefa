use crate::decl::{Holder, Widget};
use crate::render::{Render, Shade};

// Every impl here has a self type declared in decl.rs, never in this file, so
// `entity_span_named` finds no in-file owner entity.
impl Render for Widget {
    fn render(&self) -> u32 {
        self.tag
    }
}

impl Shade for Holder<Widget> {
    fn shade(&self) -> u32 {
        0
    }
}

impl Holder<Widget> {
    pub fn tag(&self) -> u32 {
        self.item.tag
    }
}
