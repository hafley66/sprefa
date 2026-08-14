#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MaybeTextNone {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MaybeTextSome {
    pub id: i64,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MaybeTextTag {
    pub id: i64,
    pub tag: String,
}
