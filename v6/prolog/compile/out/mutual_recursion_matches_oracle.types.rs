#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Clock {
    pub col1: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Even {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Odd {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Seed {
    pub value: i64,
}
