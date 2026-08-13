#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Active {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Disabled {
    pub name: String,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    pub name: String,
}
