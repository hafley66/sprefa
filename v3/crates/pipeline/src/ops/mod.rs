//! Built-in ops on the v3 pipeline::Op trait.
//!
//! Each op file is one type implementing `Op`. Ops own their own
//! ergonomics — name, slot decls (later), diagnostics (later). The
//! framework just runs them.
//!
//! Inventory (this slice):
//!   - capture_write: `> $TARGET` — name the active span.
//!   - void:          `> void`     — drop the cursor.

pub mod capture_write;
pub mod glob;
pub mod re;
pub mod str_op;
pub mod void;

pub use capture_write::CaptureWriteOp;
pub use glob::{CompiledGlob, GlobOp, Segment as GlobSegment};
pub use re::{CompiledRe, ReOp};
pub use str_op::{StrOp, StrValue};
pub use void::VoidOp;
