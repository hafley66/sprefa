//! Wire types for the TSI envelope. No extraction logic, no relation registry:
//! arity and argument kinds are the decoder's job, and it lands separately.

use serde::{Deserialize, Serialize};

/// A consumer reading a protocol number it does not know must stop, never
/// guess a shape.
pub const PROTOCOL_VERSION: u32 = 1;

/// `Syntax` is the parse alone; `Semantic` is a native checker walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Syntax,
    Semantic,
}

/// `scope` is the content digests the run read, so two runs over the same
/// bytes are comparable without re-reading them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOut {
    pub run: u32,
    pub mode: Mode,
    pub tool: String,
    pub version: String,
    pub scope: Vec<String>,
}

/// Externally tagged, so the JSON is the tag object the protocol spells:
/// `{"id":4}`, `{"span":["blake3:...",10,14]}`, `{"text":"x"}`, `{"int":-1}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arg {
    Id(u32),
    /// Content digest, then the half-open byte offsets.
    Span(String, u32, u32),
    Text(String),
    Int(i64),
    Atom(String),
}

/// One relation row: run-local ordinal, `<ns>.<name>`, arguments in
/// declaration order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactOut {
    pub fact: u32,
    pub relation: String,
    pub args: Vec<Arg>,
}

/// One row per (fact, run), which is what lets a syntax guess and a checker
/// answer coexist on one fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessOut {
    pub fact: u32,
    pub run: u32,
    pub method: Method,
}

/// `complete` claims every reachable row was emitted, so absence from the
/// relation is meaningful. A syntax run never claims it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageOut {
    pub run: u32,
    pub relation: String,
    #[serde(rename = "coverage", with = "coverage_flag")]
    pub complete: bool,
}

/// Mandatory beside a semantic run's partial coverage. A syntax run emits
/// none: partial is its whole mode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticOut {
    pub run: u32,
    pub relation: String,
    pub detail: String,
}

/// The `crate::types::ResolutionOrigin` vocabulary plus the three producers
/// that answer no single site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    SameFile,
    CorpusUnique,
    ModulePlane,
    Checker,
    AliasChain,
    Param,
    Receiver,
    SelfType,
    IfaceImpl,
    Decorator,
    Subscript,
    ReturnCall,
    Scip,
    Unresolved,
    /// The parse alone named the row; no resolution was attempted.
    Parse,
    /// A native checker enumerated a relation rather than answering one site.
    CheckerWalk,
    /// The row arrived through the reverse door.
    Foreign,
}

/// `coverage` is a two-valued word, not a boolean: `partial` and `complete`
/// are the vocabulary a consumer matches on, and a JSON `true` says neither.
mod coverage_flag {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(complete: &bool, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(if *complete { "complete" } else { "partial" })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<bool, D::Error> {
        let word = String::deserialize(input)?;
        match word.as_str() {
            "complete" => Ok(true),
            "partial" => Ok(false),
            other => Err(serde::de::Error::custom(format!(
                "coverage is `partial` or `complete`, got `{other}`"
            ))),
        }
    }
}
