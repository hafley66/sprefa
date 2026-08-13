#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DivisionInput {
    pub label: String,
    pub numerator: i64,
    pub denominator: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Probe {
    pub label: String,
    pub quotient: i64,
    pub remainder: i64,
}
