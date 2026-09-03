pub struct Widget {
    pub id: u32,
}

impl Widget {
    pub fn new() -> Self {
        Widget { id: 1 }
    }
    pub fn build(&self) -> u32 {
        self.id
    }
    pub fn from_self() -> Self {
        Self::new()
    }
}

pub trait Greet {
    fn hello(&self) -> u32;
}

impl Greet for Widget {
    fn hello(&self) -> u32 {
        self.build()
    }
}

pub struct Holder {
    pub w: Widget,
}

impl Holder {
    pub fn uses_field(&self) -> u32 {
        self.w.build()
    }
}

pub fn param_typed(w: &Widget) -> u32 {
    w.build()
}

pub fn let_typed() -> u32 {
    let w: Widget = Widget::new();
    w.build()
}

pub fn one_hop() -> Result<u32, ()> {
    let w = make_widget()?;
    w.build()
}

pub fn make_widget() -> Result<Widget, ()> {
    Ok(Widget::new())
}

pub fn unknown_recv() -> u32 {
    let w = mystery().wrap().into();
    w.build()
}

fn mystery() -> u32 {
    0
}
