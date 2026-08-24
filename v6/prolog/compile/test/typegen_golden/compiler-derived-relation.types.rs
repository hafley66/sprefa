#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "tag", content = "value", rename_all = "snake_case")]
pub enum DlOption<T> {
    None,
    Some(T),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Holder {
    pub value: GenPartialUser9d7a703929b72789,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenPartialUser9d7a703929b72789 {
    pub id: DlOption<i64>,
    pub name: DlOption<String>,
}
