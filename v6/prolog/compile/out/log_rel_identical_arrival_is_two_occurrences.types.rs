#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ev {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fired {
    pub name: String,
}
