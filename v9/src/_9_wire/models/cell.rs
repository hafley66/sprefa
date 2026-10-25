use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(non_camel_case_types)]
pub enum Cell {
  text(String),
  int(i64),
  real(f64),
}
