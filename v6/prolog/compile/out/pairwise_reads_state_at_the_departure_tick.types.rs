#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Reading {
    pub sensor: String,
    pub previous: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub sensor: String,
    pub previous: i64,
    pub current: i64,
}
