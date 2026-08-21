// Ground truth for the dead-module rail. Every module below is labelled with
// what rustc's dead_code lint says and what the rail must say. A run that
// disagrees with a label is a rail defect, in one tool or the other.
pub mod live_pub;
pub mod dead_pub;
pub mod live_trait_impls;
mod live_private;
mod dead_private;
mod dead_trait_impls;

use live_trait_impls::{Paint, Red};

pub fn entry() -> usize {
    let brush: &dyn Paint = &Red;
    live_pub::exported_one() + live_private::helper_one() + brush.paint()
}
