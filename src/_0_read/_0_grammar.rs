//! The generated DL7 tree-sitter parser, compiled and linked by `build.rs`.
//! A safe `tree_sitter::Language` needs the `tree-sitter-language` crate.

use tree_sitter::ffi::TSLanguage;

extern "C" {
    pub fn tree_sitter_dl7() -> *const TSLanguage;
}

pub fn language_ptr() -> *const TSLanguage {
    unsafe { tree_sitter_dl7() }
}
