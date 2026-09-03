// A second module of the same crate, declaring its own type and its own trait.
// Supplying `lib.rs` alone must reach neither impl below.

pub struct Other;

impl core::fmt::Debug for Other {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        out.write_str("Other")
    }
}

pub trait Elsewhere {}

impl Elsewhere for Other {}
