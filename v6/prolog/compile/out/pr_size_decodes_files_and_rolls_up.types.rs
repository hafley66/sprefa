#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirFile {
    pub dir: String,
    pub path: String,
    pub adds: i64,
    pub dels: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DirSize {
    pub dir: String,
    pub adds: i64,
    pub dels: i64,
    pub files: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FilesResp {
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InDir {
    pub path: String,
    pub dir: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrFile {
    pub path: String,
    pub adds: i64,
    pub dels: i64,
}
