//! Built-in ops on the v3 pipeline::Op trait.
//!
//! Each op file is one type implementing `Op`. Ops own their own
//! ergonomics — name, slot decls (later), diagnostics (later). The
//! framework just runs them.
//!
//! Inventory (this slice):
//!   - capture_write: `> $TARGET` — name the active span.
//!   - void:          `> void`     — drop the cursor.
//!   - ans_ref:       `$$`         — resolve implicit binding (no-op pass-through with the binding's range).

pub mod capture_write;
pub mod void;
pub mod ans_ref;

pub use capture_write::CaptureWriteOp;
pub use void::VoidOp;
pub use ans_ref::AnsRefOp;
