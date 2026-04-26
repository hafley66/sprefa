//! Built-in ops on the v3 pipeline::Op trait.
//!
//! Each op file is one type implementing `Op`. Ops own their own
//! ergonomics — name, slot decls (later), diagnostics (later). The
//! framework just runs them.
//!
//! Inventory (this slice):
//!   - void:          `> void`     — drop the cursor.

pub mod ast_grep;
pub mod comment;
pub mod cursor_ref;
pub mod fs;
pub mod glob;
pub mod json;
pub mod print;
pub mod re;
pub mod read;
pub mod repo;
pub mod rev;
pub mod str_op;
pub mod void;
pub mod write_file;

pub use ast_grep::AstGrepOp;
pub use comment::CommentOp;
pub use cursor_ref::CursorRefOp;
pub use fs::{FsMode, FsOp};
pub use glob::GlobOp;
pub use json::JsonOp;
pub use print::PrintOp;
pub use read::ReadOp;
pub use re::ReOp;
pub use repo::{RepoMode, RepoOp};
pub use rev::{RevMode, RevOp};
pub use str_op::{StrOp, StrValue};
pub use void::VoidOp;
pub use write_file::WriteFileOp;
