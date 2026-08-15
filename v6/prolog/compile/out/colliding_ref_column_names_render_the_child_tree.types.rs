#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Holder {
    pub id: i64,
    pub nested: OuterPair,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InnerPair {
    pub first: i64,
    pub second: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OuterPair {
    pub first: InnerPair,
    pub second: InnerPair,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Touched {
    pub id: i64,
}
