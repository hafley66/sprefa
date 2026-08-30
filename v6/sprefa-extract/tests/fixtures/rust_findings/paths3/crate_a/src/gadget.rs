use crate::traits_mod::{Polish, Sand};

pub struct Gadget {
    pub id: u32,
}

impl Polish for Gadget {
    fn polish(&self) -> u32 {
        self.id
    }
}

impl Sand for Gadget {
    fn polish(&self) -> u32 {
        0
    }

    fn from(&self) -> u32 {
        self.id
    }
}

pub fn gadget_user(g: &Gadget) -> u32 {
    g.polish()
}
