#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Adjusted {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub name: String,
    pub value: f64,
}
