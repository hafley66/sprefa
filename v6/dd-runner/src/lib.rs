use serde::Deserialize;
use serde_json::Value;

#[path = "0_dl7_types.rs"]
pub mod dl7_types;
pub mod kernel;

#[derive(Deserialize)]
pub struct Plan {
    pub ddl: Vec<String>,
    pub rels: Vec<Rel>,
    pub rules: Vec<Rule>,
    pub initial: Vec<Row>,
    pub schedule: Vec<Vec<SignedRow>>,
    pub tick_order: Vec<String>,
    #[serde(default)]
    pub operators: Vec<Value>,
}

#[derive(Clone, Deserialize)]
pub struct Rel {
    pub name: String,
    pub columns: Vec<String>,
    pub select_all: String,
}

#[derive(Clone, Deserialize, PartialEq)]
pub struct Rule {
    pub id: String,
    pub head: String,
    pub delete: String,
    pub inserts: Vec<String>,
}

#[derive(Clone, Deserialize)]
pub struct Row {
    pub rel: String,
    pub values: Vec<Value>,
}

#[derive(Clone, Deserialize)]
pub struct SignedRow {
    pub sign: i8,
    #[serde(flatten)]
    pub row: Row,
}
