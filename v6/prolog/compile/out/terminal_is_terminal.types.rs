#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamEnd {
    pub args: String,
    pub col2: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamItem {
    pub args: String,
    pub col2: i64,
    pub col3: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamStatus {
    pub args: String,
    pub col2: String,
}
