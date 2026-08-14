#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Prefix {
    pub text_value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Prefixed {
    pub name: String,
    pub prefix: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sym {
    pub name: String,
}
