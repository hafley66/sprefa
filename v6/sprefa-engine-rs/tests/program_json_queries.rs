// TEST: an IR document emitted before `queries` existed still parses, and the
// field reads empty. A required field here would reject every snapshot in the
// tree the day the emitter side lands.

use sprefa_engine_rs::types::ProgramJson;

fn minimal_program_json(extra: &str) -> String {
    format!(
        r#"{{"name":"probe","intern_mode":"dict","ddl":[],"rel_columns":{{}},
            "rel_column_types":{{}},"arrival_targets":[],"boot":[],
            "final_select":{{}},"arrival_templates":{{}},"relations":[],
            "edges":[],"levels":[],"retentions":[],"reconcile_every_tick":false,
            "ir_version":1{extra}}}"#
    )
}

#[test]
fn a_program_json_without_queries_parses_to_an_empty_list() {
    let without: ProgramJson =
        serde_json::from_str(&minimal_program_json("")).expect("parse without queries");
    assert!(without.queries.is_empty());

    let with: ProgramJson = serde_json::from_str(&minimal_program_json(
        r#","queries":["adult","big_city_person"]"#,
    ))
    .expect("parse with queries");
    assert_eq!(with.queries, vec!["adult", "big_city_person"]);
}
