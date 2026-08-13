#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Measure {
    pub name: String,
    pub whole: i64,
    pub fraction: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Total {
    pub name: String,
    pub value: f64,
}
