pub struct Beta {
    pub id: u32,
}

impl Beta {
    pub fn new() -> Self {
        Beta { id: 2 }
    }

    pub fn tick(&self) -> u32 {
        self.id
    }
}

pub fn beta_one_hop() -> u32 {
    let b = Beta::new();
    b.tick()
}
