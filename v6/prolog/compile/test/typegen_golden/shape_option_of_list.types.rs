#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "tag", content = "value", rename_all = "snake_case")]
pub enum DlOption<T> {
    None,
    Some(T),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub id: i64,
    pub tag_values: DlOption<Vec<String>>,
    pub grid_values: DlOption<Vec<Vec<String>>>,
    pub note: DlOption<String>,
    pub maybe_tag_values: Vec<DlOption<String>>,
}
