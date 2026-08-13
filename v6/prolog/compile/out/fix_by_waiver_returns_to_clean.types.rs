#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnyDiag {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CheckExit {
    pub name: String,
    pub col2: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Diag {
    pub path: String,
    pub line_no: i64,
    pub severity: String,
    pub code: String,
    pub col5: String,
    pub col6: String,
    pub col7: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagStage {
    pub code: String,
    pub stage: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnBaseline {
    pub path: String,
    pub allowed: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnCount {
    pub path: String,
    pub hits: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnCounted {
    pub path: String,
    pub line_no: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnHit {
    pub path: String,
    pub line_no: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnWaived {
    pub path: String,
    pub line_no: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnWaiverLine {
    pub path: String,
    pub waiver_line: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GateBlocked {
    pub stage: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GateExit {
    pub stage: String,
    pub col2: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GateThreshold {
    pub stage: String,
    pub min_rank: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Program {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeverityRank {
    pub severity: String,
    pub rank: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WaiverBlockComment {
    pub path: String,
    pub waiver_line: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WaiverTrailingComment {
    pub path: String,
    pub waiver_line: i64,
}
