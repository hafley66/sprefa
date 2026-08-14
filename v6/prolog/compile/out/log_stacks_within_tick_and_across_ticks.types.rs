#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Heard {
    pub item: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HeardCount {
    pub item: String,
}
