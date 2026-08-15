pub trait JsonEncodable {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pair<T: JsonEncodable> {
    pub first: T,
    pub second: T,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Box<V> {
    pub value: V,
    pub label: String,
}
