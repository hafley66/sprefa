#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Box {
    pub id: i64,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub items: Vec<String>,
}
