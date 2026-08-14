#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultError {
    pub id: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultOk {
    pub id: i64,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultTag {
    pub id: i64,
    pub tag: String,
}
