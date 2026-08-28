pub trait JsonEncodable {}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mapper<In: JsonEncodable, Out: JsonEncodable> {
    pub input: In,
    pub r#return: Out,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GenMapperIntText27b8a56119fbf234 {
    pub input: i64,
    pub r#return: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Carry {
    pub id: i64,
    pub applied: GenMapperIntText27b8a56119fbf234,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Conversion {
    pub id: i64,
    pub applied: GenMapperIntText27b8a56119fbf234,
}
