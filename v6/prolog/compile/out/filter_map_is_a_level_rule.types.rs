#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Doubled {
    pub name: String,
    pub out: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Reading {
    pub name: String,
    pub value: i64,
}
