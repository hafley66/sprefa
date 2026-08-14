#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Demanded {
    pub target: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenScope {
    pub session_id: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteChange {
    pub session_id: String,
    pub route_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteRow {
    pub route_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteView {
    pub route_id: String,
    pub body: String,
}
