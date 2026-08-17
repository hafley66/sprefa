pub trait JsonEncodable {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry<Key: JsonEncodable, Value> {
    pub key: Key,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenEntryTextIntA6c3f6c7e60e6b95 {
    pub key: String,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub slot: GenEntryTextIntA6c3f6c7e60e6b95,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    pub id: i64,
    pub slot: GenEntryTextIntA6c3f6c7e60e6b95,
}
