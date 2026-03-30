pub mod extract;
pub mod files;
pub mod normalize;

pub use extract::{extract, extract_files, ExtractedFile};
pub use files::{DiffResult, diff_files};
pub use normalize::{normalize, normalize2};
