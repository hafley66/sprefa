pub struct Widget {
    pub size: u32,
}

pub trait Reset {
    fn reset(&self) -> Widget;
}

impl Widget {
    pub fn build() -> Widget {
        Widget { size: 0 }
    }

    pub fn reset(&self) -> Widget {
        Widget { size: self.size }
    }
}

impl Reset for Widget {
    fn reset(&self) -> Widget {
        Widget { size: 0 }
    }
}

pub fn widget_user(w: &Widget) -> u32 {
    let next = w.reset();
    next.size
}
