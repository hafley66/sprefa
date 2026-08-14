#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fee {
    pub account: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FeeStats {
    pub account: String,
    pub next: i64,
}
