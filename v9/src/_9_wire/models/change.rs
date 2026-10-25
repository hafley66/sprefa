use serde::Deserialize;
use serde::Serialize;

use crate::_9_wire::models::cell::Cell;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
  pub rel: String,
  pub cells: Vec<Option<Cell>>,
  pub diff: i64,
}
