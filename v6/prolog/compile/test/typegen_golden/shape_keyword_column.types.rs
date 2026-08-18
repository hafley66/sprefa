#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Widget {
    pub r#type: String,
    pub r#match: String,
    pub r#ref: String,
    pub label: String,
}
