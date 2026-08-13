#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamEnd {
    pub col1: String,
    pub col2: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamItem {
    pub col1: String,
    pub col2: i64,
    pub col3: String,
}
