use serde::{Deserialize, Serialize};

// Generated Rust signature: DlOption<DlOption<T>>. The JSON timeline keeps
// outer absence, present inner absence, and present value as separate states.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tag", content = "value", rename_all = "snake_case")]
enum DlOption<T> {
    None,
    Some(T),
}

#[test]
fn nested_option_serde_roundtrips_every_presence_state() {
    let states = [
        (DlOption::<DlOption<i64>>::None, r#"{"tag":"none"}"#),
        (
            DlOption::Some(DlOption::None),
            r#"{"tag":"some","value":{"tag":"none"}}"#,
        ),
        (
            DlOption::Some(DlOption::Some(7)),
            r#"{"tag":"some","value":{"tag":"some","value":7}}"#,
        ),
    ];

    for (state, json) in states {
        assert_eq!(serde_json::to_string(&state).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<DlOption<DlOption<i64>>>(json).unwrap(),
            state
        );
    }
}
