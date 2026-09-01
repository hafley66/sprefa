pub mod decoys;
pub mod widget;

use std::path::PathBuf;

use crate::widget::*;

/// Two corpus files declare `Widget`, so `unique_declared_type` declines and
/// the glob binding is what reaches `widget.rs`.
pub struct Holder {
    pub shown: Widget,
}

pub use crate::widget::Widget as Gadget;

/// No corpus type is DECLARED `Gadget`: the tier answers under the declared
/// name, which this file never spells.
pub struct Renamed {
    pub shown: Gadget,
}

/// `PathBuf` is std and `decoys.rs` declares the only corpus type of the name,
/// so a name match binds the decoy; the checker's `external` answer suppresses it.
pub struct Located {
    pub location: PathBuf,
}

/// One file naming `Config` two ways: the glob binding and the decoy path. The
/// checker's per-file map keys on the NAME alone, so the two answers collide.
pub struct Mixed {
    pub from_glob: Config,
    pub from_decoy: decoys::Config,
}
