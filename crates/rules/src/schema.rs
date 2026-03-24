use schemars::schema_for;

use crate::types::RuleSet;

/// Generate the JSON Schema for the sprefa rules format.
pub fn generate_schema() -> serde_json::Value {
    let schema = schema_for!(RuleSet);
    serde_json::to_value(schema).expect("schema serialization cannot fail")
}

/// Generate the JSON Schema as a pretty-printed string.
pub fn generate_schema_string() -> String {
    serde_json::to_string_pretty(&generate_schema()).expect("schema serialization cannot fail")
}
