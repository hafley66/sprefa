#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Matched {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Source {
    pub text: String,
}
