#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShapeCircle {
    pub id: i64,
    pub radius: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShapeSquare {
    pub id: i64,
    pub side: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShapeTag {
    pub id: i64,
    pub tag: String,
}
