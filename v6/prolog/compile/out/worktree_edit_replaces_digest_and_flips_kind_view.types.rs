#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RustFile {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeEdit {
    pub path: String,
    pub digest: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeFile {
    pub path: String,
    pub digest: String,
    pub kind: String,
}
