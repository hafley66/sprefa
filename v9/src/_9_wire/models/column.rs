use serde::Deserialize;
use serde::Serialize;

use crate::models::col_type::ColType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
  pub name: String,
  pub col_type: ColType,
}
