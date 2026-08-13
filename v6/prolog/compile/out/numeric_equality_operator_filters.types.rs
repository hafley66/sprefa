#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Matched {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Row {
    pub value: i64,
}
