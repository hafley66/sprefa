#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bump {
    pub name: String,
    pub extra: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OverBudget {
    pub name: String,
    pub sum: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    pub name: String,
    pub base: i64,
    pub col3: String,
}
