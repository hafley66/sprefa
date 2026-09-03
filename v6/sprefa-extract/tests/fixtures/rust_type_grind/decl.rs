pub struct Widget {
    pub tag: u32,
}

pub struct Local {
    pub tag: u32,
}

// The self type is declared in this file: the row is `Local -> Local`, the
// oracle's same-file self-edge.
impl Local {
    pub fn tag(&self) -> u32 {
        self.tag
    }
}
