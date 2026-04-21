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
pub mod void;

pub use capture_write::CaptureWriteOp;
pub use void::VoidOp;
