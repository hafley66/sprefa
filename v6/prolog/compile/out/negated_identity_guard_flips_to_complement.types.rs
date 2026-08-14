#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Distinct {
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pair {
    pub left: String,
    pub right: String,
}
