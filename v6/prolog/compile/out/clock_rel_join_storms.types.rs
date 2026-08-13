#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagHistory {
    pub path: String,
    pub line: i64,
    pub code: String,
    pub opened_at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagSeen {
    pub path: String,
    pub line: i64,
    pub code: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: i64,
    pub code: String,
    pub col4: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileLine {
    pub path: String,
    pub line: i64,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ratchet {
    pub col1: String,
    pub col2: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TickRel {
    pub at: i64,
}
