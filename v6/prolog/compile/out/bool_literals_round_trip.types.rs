#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Flag {
    pub name: String,
    pub enabled: bool,
}
