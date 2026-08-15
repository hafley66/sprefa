#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Grid {
    pub id: i64,
    pub rows: Vec<Vec<String>>,
}
