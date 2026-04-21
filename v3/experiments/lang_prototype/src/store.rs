use super::cursor::SprfPath;
use super::lower::Scalar;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct TagRow {
    pub kind: Arc<str>,
    pub value: Scalar,
    pub path: SprfPath,
}

#[derive(Clone, Debug, Default)]
pub struct Store {
    pub tags: Vec<TagRow>,
}

impl Store {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn write_tag(&mut self, kind: Arc<str>, value: Scalar, path: SprfPath) {
        self.tags.push(TagRow { kind, value, path });
    }
    pub fn query_kind(&self, kind: &str) -> Vec<&TagRow> {
        self.tags.iter().filter(|r| r.kind.as_ref() == kind).collect()
    }
    pub fn query_all(&self) -> Vec<&TagRow> {
        self.tags.iter().collect()
    }
}
