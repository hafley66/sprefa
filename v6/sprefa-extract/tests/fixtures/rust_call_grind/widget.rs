pub struct Widget;

impl Widget {
    pub fn new() -> Self {
        Widget
    }

    pub fn tick(&self) -> Gauge {
        Gauge
    }
}

pub struct Gauge;

impl Gauge {
    pub fn read(&self) -> u32 {
        1
    }
}

pub fn chain_caller() -> u32 {
    let gauge = Widget::new().tick();
    gauge.read()
}

pub fn hop_caller() -> u32 {
    let widget = Widget::new();
    let gauge = widget.tick();
    gauge.read()
}
