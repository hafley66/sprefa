#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scaled {
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub count: i64,
}
