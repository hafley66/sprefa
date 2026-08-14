#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Text {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Titled {
    pub text: String,
    pub out: String,
}
