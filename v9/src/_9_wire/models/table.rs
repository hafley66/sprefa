use serde::Deserialize;
use serde::Serialize;

use crate::models::cell::Cell;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
  pub columns: Vec<String>,
  pub rows: Vec<Vec<Cell>>,
}
