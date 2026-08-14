#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IsProlog {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Path {
    pub name: String,
}
