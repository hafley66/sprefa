#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleDefs {
    pub path: String,
    pub defs: i64,
}
