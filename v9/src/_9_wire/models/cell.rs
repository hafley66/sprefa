use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
  pub text: Option<String>,
  pub int: Option<i64>,
}
