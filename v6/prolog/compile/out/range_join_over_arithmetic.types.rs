#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnHit {
    pub path: String,
    pub line_number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnWaived {
    pub path: String,
    pub line_number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnWaiverLine {
    pub path: String,
    pub waiver_line: i64,
}
