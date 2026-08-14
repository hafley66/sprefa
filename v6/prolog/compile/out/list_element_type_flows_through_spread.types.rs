#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Symbol {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SymbolParts {
    pub name: String,
    pub parts: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SymbolWord {
    pub name: String,
    pub word: String,
}
