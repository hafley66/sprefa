#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Echoed {
    pub name: String,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Label {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Labelled {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Payload {
    pub name: String,
    pub body: serde_json::Value,
}
