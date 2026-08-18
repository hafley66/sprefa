pub trait JsonEncodable {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pair<T: JsonEncodable> {
    pub first: T,
    pub second: T,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenPairInt8b7ec0fa0e1f9d69 {
    pub first: i64,
    pub second: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub endpoints: GenPairInt8b7ec0fa0e1f9d69,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub id: i64,
    pub endpoints: GenPairInt8b7ec0fa0e1f9d69,
}
