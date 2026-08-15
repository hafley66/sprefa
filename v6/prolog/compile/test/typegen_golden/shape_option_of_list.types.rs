#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub id: i64,
    pub tag_values: Option<Vec<String>>,
    pub grid_values: Option<Vec<Vec<String>>>,
    pub note: Option<String>,
    pub maybe_tag_values: Vec<Option<String>>,
}
