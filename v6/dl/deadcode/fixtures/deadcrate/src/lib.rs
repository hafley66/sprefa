// Ground truth for the dead-module rail. Every module below is labelled with
// what rustc's dead_code lint says and what the rail must say. A run that
// disagrees with a label is a rail defect, in one tool or the other.
pub mod live_pub;
pub mod dead_pub;
pub mod live_trait_impls;
pub mod ambiguous_owner;
pub mod ambiguous_other;
pub mod test_only_defs;
mod live_private;
mod dead_private;
mod dead_trait_impls;
mod called_only_from_tests;
mod mixed_call_sites;

use live_trait_impls::{Paint, Red};
use ambiguous_owner::Widget;
use ambiguous_other::Gauge;

pub fn entry() -> usize {
    let brush: &dyn Paint = &Red;
    live_pub::exported_one() + live_private::helper_one() + brush.paint()
        + Widget.refresh() + Gauge.refresh()
}
