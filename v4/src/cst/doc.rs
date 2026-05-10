//! `Doc`: a host parse plus its (recursively) injected sub-trees.
//! `DocId`: the host-document identity (typically a file URI).

use std::sync::Arc;

use crate::cst::injected::Injected;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct DocId(pub Arc<str>);

impl DocId {
    pub fn new(s: impl Into<Arc<str>>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct Doc {
    pub doc_id: DocId,
    pub source: Arc<str>,
    pub tree: tree_sitter::Tree,
    pub injected: Vec<Injected>,
    pub version: i32,
}
