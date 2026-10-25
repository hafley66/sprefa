use serde::Deserialize;
use serde::Serialize;

use crate::models::column::Column;
use crate::models::rel_kind::RelKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
  pub name: String,
  pub cols: Vec<Column>,
  pub kind: RelKind,
  pub service: Option<String>,
}
