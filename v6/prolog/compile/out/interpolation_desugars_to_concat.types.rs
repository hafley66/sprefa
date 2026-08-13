#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EprintlnHit {
    pub path: String,
    pub line_number: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub path: String,
    pub line_number: i64,
    pub text: String,
}
