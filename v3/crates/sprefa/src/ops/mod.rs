pub mod rule;
pub mod repo;
pub mod rev;
pub mod fs;
pub mod read;
pub mod json;
pub mod cursor_ref;
pub mod line;
pub mod md;
pub mod ast_grep;
pub mod marker;

pub use rule::RuleFactory;
pub use repo::RepoFactory;
pub use rev::RevFactory;
pub use fs::FsFactory;
pub use read::ReadFactory;
pub use json::{JsonFactory, JsonTree, JSON_TREE};
pub use cursor_ref::CursorRefFactory;
pub use line::LineFactory;
pub use md::MdFactory;
pub use ast_grep::AstGrepFactory;
pub use marker::MarkerFactory;

/// Build a registry with all standard ops registered. Use this everywhere
/// instead of manually listing factories — adding a new op only requires
/// touching this one function.
pub fn default_registry() -> crate::registry::OperatorRegistry {
    let mut r = crate::registry::OperatorRegistry::new();
    r.register(std::sync::Arc::new(RuleFactory));
    r.register(std::sync::Arc::new(RepoFactory));
    r.register(std::sync::Arc::new(RevFactory));
    r.register(std::sync::Arc::new(FsFactory));
    r.register(std::sync::Arc::new(ReadFactory));
    r.register(std::sync::Arc::new(JsonFactory));
    r.register(std::sync::Arc::new(CursorRefFactory));
    r.register(std::sync::Arc::new(LineFactory));
    r.register(std::sync::Arc::new(MdFactory));
    r.register(std::sync::Arc::new(AstGrepFactory));
    r.register(std::sync::Arc::new(MarkerFactory));
    r
}
