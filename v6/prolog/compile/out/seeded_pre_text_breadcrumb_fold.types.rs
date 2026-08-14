#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Breadcrumb {
    pub path: String,
    pub next: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub path: String,
    pub piece: String,
}
