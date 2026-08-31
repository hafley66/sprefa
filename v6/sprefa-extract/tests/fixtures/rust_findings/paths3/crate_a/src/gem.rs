use crate::traits_mod::Sand;

pub struct Gem {
    pub grade: u32,
}

impl From<u32> for Gem {
    fn from(value: u32) -> Gem {
        Gem { grade: value }
    }
}

impl Sand for Gem {
    fn polish(&self) -> u32 {
        self.grade
    }

    fn from(&self) -> u32 {
        0
    }
}
