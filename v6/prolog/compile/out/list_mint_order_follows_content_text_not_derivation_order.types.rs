#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FruitParts {
    pub name: String,
    pub parts: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FruitText {
    pub name: String,
    pub body: String,
}
