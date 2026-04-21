//! Extractor trait. Js / Rs / Sprf all implement this.

use crate::types::FilePath;
use crate::writer::RuleTableSpec;
use crate::op::Pipeline;

pub trait Extractor: Send + Sync {
    fn name(&self) -> &'static str;
    fn accepts(&self, file: &FilePath) -> bool;
    fn chain(&self)  -> &Pipeline;
    fn specs(&self)  -> &[RuleTableSpec];
}

/// Hardcoded vs sprf-derived discriminator for introspection / metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractorKind {
    Hardcoded,
    Sprf,
}

pub fn kind_of(name: &str) -> ExtractorKind {
    if name.starts_with("sprf:") { ExtractorKind::Sprf } else { ExtractorKind::Hardcoded }
}
