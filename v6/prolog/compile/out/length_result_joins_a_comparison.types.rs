#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Long {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Text {
    pub text: String,
}
