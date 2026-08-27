#[path = "vendor/legacy.rs"]
pub mod legacy;

pub fn shout() -> u32 {
    super::a::f() + legacy::two()
}
