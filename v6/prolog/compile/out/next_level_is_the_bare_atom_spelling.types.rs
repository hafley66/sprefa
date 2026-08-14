#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seen {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub value: i64,
}
