//! sprefa::reader — re-export shim over `spine::reader`.
//!
//! The `Reader` trait lives in spine so ops can accept `Arc<dyn Reader>`
//! without a sprefa dep. Concrete readers (`GitBlobReader`, `MemReader`,
//! `BufferOverlay`) stay in `sprefa::readers::*`.
pub use spine::reader::*;
