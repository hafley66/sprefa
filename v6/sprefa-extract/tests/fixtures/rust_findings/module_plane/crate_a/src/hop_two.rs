//! module plane fixture: a second `pub use` hop over `hop_one`, hops=2 total.
pub use crate::hop_one::reexported_fn as reexported_fn_two;
