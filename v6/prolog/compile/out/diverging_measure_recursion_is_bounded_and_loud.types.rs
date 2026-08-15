#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Counter {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeedNumber {
    pub value: i64,
}
