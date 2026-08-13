#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Named {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tagged {
    pub id: i64,
    pub tag: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TagsHolder {
    pub id: i64,
}
