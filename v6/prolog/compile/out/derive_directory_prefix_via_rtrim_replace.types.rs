#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Directory {
    pub file: String,
    pub dir: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FilePath {
    pub file: String,
}
