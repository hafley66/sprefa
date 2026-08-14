#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagHistory {
    pub path: String,
    pub line: i64,
    pub code: String,
    pub opened_at: i64,
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
pub struct HookWindow {
    pub turn: String,
    pub since: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LintCount {
    pub code: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ratchet {
    pub code: String,
    pub allowed: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TurnDiag {
    pub turn: String,
    pub path: String,
    pub line: i64,
    pub code: String,
    pub opened_at: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnratchetedLint {
    pub code: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Violation {
    pub code: String,
    pub count: i64,
    pub allowed: i64,
}
