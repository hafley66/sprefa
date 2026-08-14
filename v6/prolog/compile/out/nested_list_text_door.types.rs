#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grid {
    pub id: i64,
    pub rows: i64,
}
