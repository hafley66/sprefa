#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FoundAt {
    pub text: String,
    pub out: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Text {
    pub text: String,
}
