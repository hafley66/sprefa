#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GenResultHostErrorBoopResponse5731b3aa340db474 {
    Err { error: HostError },
    Ok { value: BoopResponse },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GenResultParseErrorSyntaxTree0284bcd3105168e0 {
    Err { error: ParseError },
    Ok { value: SyntaxTree },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoopResponse {
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Compile {
    pub id: i64,
    pub outcome: GenResultParseErrorSyntaxTree0284bcd3105168e0,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fetch {
    pub id: i64,
    pub outcome: GenResultHostErrorBoopResponse5731b3aa340db474,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HostError {
    pub code: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParseError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxTree {
    pub root: String,
}
