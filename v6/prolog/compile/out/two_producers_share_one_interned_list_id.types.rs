#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LeftParts {
    pub name: String,
    pub parts: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LeftText {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RightParts {
    pub name: String,
    pub parts: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RightText {
    pub name: String,
    pub body: String,
}
