pub trait JsonEncodable {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Couple<Left: JsonEncodable, Right: JsonEncodable> {
    #[serde(skip)]
    pub phantom: std::marker::PhantomData<fn() -> (Left, Right)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Wrap<T: JsonEncodable> {
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenCoupleWrapIntWrapTextFea7bde20e4f244e {
    pub first: GenWrapInt74568235536ee9d4,
    pub second: GenWrapText2bd6acc46ade78fd,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenWrapInt74568235536ee9d4 {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenWrapText2bd6acc46ade78fd {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub nested: GenCoupleWrapIntWrapTextFea7bde20e4f244e,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Index {
    pub id: i64,
    pub nested: GenCoupleWrapIntWrapTextFea7bde20e4f244e,
}
