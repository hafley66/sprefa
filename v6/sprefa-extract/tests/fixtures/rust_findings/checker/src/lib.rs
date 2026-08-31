pub mod panel;
pub mod widget;

pub fn drive() -> u32 {
    let made = widget::make();
    made.render()
}
