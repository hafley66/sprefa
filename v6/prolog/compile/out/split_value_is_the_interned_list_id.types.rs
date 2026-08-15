#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RowParts {
    pub name: String,
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RowText {
    pub name: String,
    pub body: String,
}
