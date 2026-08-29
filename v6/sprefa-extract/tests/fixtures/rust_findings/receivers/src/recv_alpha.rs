pub struct Alpha {
    pub id: u32,
}

impl Alpha {
    pub fn new() -> Self {
        Alpha { id: 1 }
    }

    pub fn tick(&self) -> u32 {
        self.id
    }
}
