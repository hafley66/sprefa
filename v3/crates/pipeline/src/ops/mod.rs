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
pub mod comment;
pub mod glob;
pub mod print;
pub mod read;
pub mod re;
pub mod repo;
pub mod rev;
pub mod fs;
pub mod str_op;
pub mod void;

pub use capture_write::CaptureWriteOp;
pub use comment::CommentOp;
pub use fs::{FsMode, FsOp};
pub use glob::GlobOp;
pub use print::PrintOp;
pub use read::ReadOp;
pub use re::ReOp;
pub use repo::{RepoMode, RepoOp};
pub use rev::{RevMode, RevOp};
pub use str_op::{StrOp, StrValue};
pub use void::VoidOp;
