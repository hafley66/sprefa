#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mirror {
    pub item: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceRow {
    pub item: String,
}
