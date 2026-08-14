#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    pub text_value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Raw {
    pub text_value: String,
}
