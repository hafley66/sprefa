#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fpath {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Loc {
    pub at: Fpath,
    pub line: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Raw {
    pub path: String,
    pub line: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    pub at: Fpath,
}
