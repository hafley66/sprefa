pub trait JsonEncodable {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span<Start: JsonEncodable, Label: JsonEncodable> {
    pub start: Start,
    #[serde(skip)]
    pub phantom: std::marker::PhantomData<fn() -> (Label,)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenSpanIntTextE5126de851365aff {
    pub start: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub extent: GenSpanIntTextE5126de851365aff,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Marker {
    pub id: i64,
    pub extent: GenSpanIntTextE5126de851365aff,
}
