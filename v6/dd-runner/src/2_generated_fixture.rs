// sprefa:auto-begin dl7-native-runtime
// generated from checked DL7 through the DBSP lowering
// program structure is native Rust; cells use the kernel Value plane

#[allow(unused_imports)]
use dd_runner::kernel::{Aggregate, LiteralEquals, Operator, Predicate, Projection};
use dd_runner::{Rel, Row};
#[allow(unused_imports)]
use std::collections::BTreeMap;

#[rustfmt::skip]
pub fn relations() -> Vec<Rel> {
    vec![
        Rel { name: String::from("Input"), columns: vec![String::from("value"), String::from("same"), String::from("tag")], select_all: String::from("") },
        Rel { name: String::from("Output"), columns: vec![String::from("value")], select_all: String::from("") },
    ]
}

#[rustfmt::skip]
pub fn initial() -> Vec<Row> {
    vec![
        Row { rel: String::from("Input"), values: vec![serde_json::Value::from(7_i64), serde_json::Value::from(7_i64), serde_json::Value::from(7_i64)] },
    ]
}

#[rustfmt::skip]
pub fn operators() -> Vec<Operator> {
    vec![
        Operator { id: String::from("map_121"), kind: String::from("map"), head: String::from("Output"), refs: vec![String::from("Input")], bindings: BTreeMap::from([(String::from("b0"), String::from("Input"))]), predicates: vec![Predicate { column_equals: Some([String::from("b0.value"), String::from("b0.same")]), literal_equals: None }, Predicate { column_equals: None, literal_equals: Some(LiteralEquals { column: String::from("b0.tag"), value: serde_json::Value::from(7_i64) }) }], projection: vec![Projection { head: String::from("value"), source: Some(String::from("b0.value")), value: None }], aggregate: None },
    ]
}
// sprefa:auto-end dl7-native-runtime

#[cfg(test)]
mod tests {
    #[test]
    fn generated_program_executes_without_a_program_decode() {
        let runtime = dd_runner::kernel::Runtime::open(
            &super::relations(),
            &super::initial(),
            super::operators(),
        )
        .unwrap();
        assert_eq!(runtime.row_count("Input"), 1);
        assert_eq!(runtime.row_count("Output"), 1);
    }
}
