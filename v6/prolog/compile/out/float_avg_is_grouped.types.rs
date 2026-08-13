#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mean {
    pub group: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Score {
    pub group: String,
    pub value: f64,
}
