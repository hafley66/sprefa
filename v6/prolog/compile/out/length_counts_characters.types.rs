#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Measured {
    pub text: String,
    pub out: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Text {
    pub text: String,
}
