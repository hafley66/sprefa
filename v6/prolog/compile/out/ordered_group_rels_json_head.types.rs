#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GroupRelsJson {
    pub group_name: String,
    pub col2: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RelCatalog {
    pub relation_name: String,
    pub group_name: String,
    pub _column_text: String,
    pub _documentation_text: String,
}
