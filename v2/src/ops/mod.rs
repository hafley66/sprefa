pub mod _0_rule;
pub mod _1_repo;
pub mod _2_rev;
pub mod _3_fs;
pub mod _4_read;
pub mod _5_json;
pub mod _6_cursor_ref;
pub mod _7_line;

pub use _0_rule::RuleFactory;
pub use _1_repo::RepoFactory;
pub use _2_rev::RevFactory;
pub use _3_fs::FsFactory;
pub use _4_read::ReadFactory;
pub use _5_json::{JsonFactory, JsonTree, JSON_TREE};
pub use _6_cursor_ref::CursorRefFactory;
pub use _7_line::LineFactory;

/// Build a registry with all standard ops registered. Use this everywhere
/// instead of manually listing factories — adding a new op only requires
/// touching this one function.
pub fn default_registry() -> crate::_10_registry::OperatorRegistry {
    let mut r = crate::_10_registry::OperatorRegistry::new();
    r.register(std::sync::Arc::new(RuleFactory));
    r.register(std::sync::Arc::new(RepoFactory));
    r.register(std::sync::Arc::new(RevFactory));
    r.register(std::sync::Arc::new(FsFactory));
    r.register(std::sync::Arc::new(ReadFactory));
    r.register(std::sync::Arc::new(JsonFactory));
    r.register(std::sync::Arc::new(CursorRefFactory));
    r.register(std::sync::Arc::new(LineFactory));
    r
}
