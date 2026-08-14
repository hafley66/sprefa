#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Exact {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub name: String,
    pub value: f64,
}
