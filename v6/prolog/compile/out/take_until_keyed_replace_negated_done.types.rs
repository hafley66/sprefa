#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Demanded {
    pub col1: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FetchResult {
    pub endpoint: String,
    pub col2: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveFetch {
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenFetch {
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Phase {
    pub endpoint: String,
    pub col2: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PollDue {
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScopeDone {
    pub endpoint: String,
}
