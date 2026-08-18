#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "tag", content = "value", rename_all = "snake_case")]
pub enum DlOption<T> {
    None,
    Some(T),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Squad {
    pub id: i64,
    pub rank: DlOption<DlOption<i64>>,
}
