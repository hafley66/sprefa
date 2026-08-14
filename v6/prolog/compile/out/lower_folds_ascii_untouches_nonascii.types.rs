#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mumbled {
    pub text: String,
    pub out: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Text {
    pub text: String,
}
