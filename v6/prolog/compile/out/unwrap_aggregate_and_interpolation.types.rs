#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangedFile {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Diag {
    pub path: String,
    pub line_no: i64,
    pub col3: String,
    pub col4: String,
    pub col5: String,
    pub col: i64,
    pub end_col: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnwrapCount {
    pub path: String,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnwrapHit {
    pub path: String,
    pub line_no: i64,
    pub col: i64,
    pub end_col: i64,
}
