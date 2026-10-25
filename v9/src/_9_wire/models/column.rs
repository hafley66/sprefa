use serde::Deserialize;
use serde::Serialize;

use crate::_9_wire::models::col_type::ColType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
  pub name: String,
  pub r#type: ColType,
}
