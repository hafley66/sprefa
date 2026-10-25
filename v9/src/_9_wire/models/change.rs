use serde::Deserialize;
use serde::Serialize;

use crate::models::cell::Cell;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
  pub rel: String,
  pub cells: Vec<Cell>,
  pub diff: i64,
}
