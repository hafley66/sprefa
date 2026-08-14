#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CalleeSetSize {
    pub left: String,
    pub left_size: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Jaccard {
    pub left: String,
    pub right: String,
    pub col3: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SharedCount {
    pub left: String,
    pub right: String,
    pub shared: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnionSize {
    pub left: String,
    pub right: String,
    pub union: i64,
}
