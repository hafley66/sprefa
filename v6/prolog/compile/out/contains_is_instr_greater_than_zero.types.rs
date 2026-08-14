#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HasSep {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sym {
    pub name: String,
}
