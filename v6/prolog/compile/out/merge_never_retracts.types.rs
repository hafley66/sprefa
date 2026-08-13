#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventA {
    pub item: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventB {
    pub item: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Out {
    pub item: String,
}
