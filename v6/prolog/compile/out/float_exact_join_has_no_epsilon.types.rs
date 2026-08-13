#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Left {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Matched {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Right {
    pub name: String,
    pub value: f64,
}
