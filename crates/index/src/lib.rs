pub mod extract;
pub mod files;
pub mod normalize;

pub use extract::{extract, extract_files, extract_rev, ExtractedFile};
pub use files::{DiffResult, GitRev, diff_files, is_semver, list_blobs_at_rev, read_git_revs};
pub use normalize::{normalize, normalize2};
