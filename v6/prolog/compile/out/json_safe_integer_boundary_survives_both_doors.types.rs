#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carried {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Measure {
    pub name: String,
    pub value: i64,
}
