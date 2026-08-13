#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Combined {
    pub value_a: String,
    pub value_b: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultA {
    pub value_a: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultB {
    pub value_b: String,
}
