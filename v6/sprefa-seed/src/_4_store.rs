//! The seam sqlite hides behind. VIEW vs MATERIALIZED is a physical form the store
//! renders; the eval strategy (from analyze) lowers to it. Bounded load/commit is
//! the trait LAW proven in the cascade lab (54MB C-heap, wavefront-bounded). A
//! future store-rocksdb / store-mem slots beside store-sqlite with no change above.

use crate::_1_feldera::{Delta, Weight};
use crate::_0_key::RelId;

pub enum PhysForm {
    Table,   // materialized, push-maintained (the cascade)
    View,    // lazy, zero storage, computed on read
}

pub trait Store {
    type Key: Ord + Copy;
    type Weight: Weight;

    /// declare a rel's physical form; EvalStrategy lowers to this.
    fn declare(&mut self, rel: RelId, form: PhysForm);

    /// pull ONLY the wavefront for a step (bounded — the proven property).
    fn load_frontier(&self, seeds: &[Self::Key]) -> Vec<(Self::Key, Self::Weight)>;

    /// commit a cascade delta in one bounded transaction.
    fn commit(&mut self, rel: RelId, delta: &Delta<Self::Key, Self::Weight>);
}
