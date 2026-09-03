use crate::decl::Widget;
use crate::render::Render;

// The self type is declared in decl.rs: the owner is an `ImplOwner` of this
// file and the head binds decl.rs's Widget.
impl Render for Widget {
    fn render(&self) -> u32 {
        self.tag
    }
}

impl Widget {
    pub fn doubled(&self) -> u32 {
        self.tag * 2
    }
}

// A qualified head is owned by its qualifier under the oracle's
// `impl_self_name`, so it mints no owner and no row here.
impl crate::decl::Local {
    pub fn tripled(&self) -> u32 {
        self.tag * 3
    }
}
