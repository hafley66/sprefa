#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Budget {
    pub team: String,
    pub col2: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spend {
    pub team: String,
    pub _item: String,
    pub cost: i64,
}
