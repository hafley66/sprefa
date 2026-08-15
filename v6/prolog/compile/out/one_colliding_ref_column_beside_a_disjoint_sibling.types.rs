#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MixedPair {
    pub first: PointPair,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PointPair {
    pub first: i64,
    pub depth: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub id: i64,
    pub nested: MixedPair,
}
