//! `SprfDiag.uri` and `SprfDiag.position` are optional, serde-skip-None,
//! serde-default-None. Older clients and recordings must still
//! deserialize and the on-wire shape must omit absent fields.

use v4::app::{SprfDiag, SprfDiagPosition};

#[test]
fn sprf_diag_deserializes_without_uri_or_position() {
    let json = r#"{"severity":"error","code":"x","message":"m"}"#;
    let d: SprfDiag = serde_json::from_str(json).unwrap();
    assert!(d.uri.is_none());
    assert!(d.position.is_none());
    assert!(d.lo.is_none());
    assert!(d.hi.is_none());
}

#[test]
fn sprf_diag_serializes_omits_none_uri_and_position() {
    let d = SprfDiag {
        lo: None,
        hi: None,
        severity: "error".into(),
        code: "x".into(),
        message: "m".into(),
        uri: None,
        position: None,
    };
    let json = serde_json::to_string(&d).unwrap();
    assert!(!json.contains("\"uri\""), "uri leaked into wire: {json}");
    assert!(
        !json.contains("\"position\""),
        "position leaked into wire: {json}",
    );
}

#[test]
fn sprf_diag_position_round_trip() {
    let d = SprfDiag {
        lo: Some(2),
        hi: Some(5),
        severity: "warning".into(),
        code: "c".into(),
        message: "msg".into(),
        uri: Some("file:///a.rs".into()),
        position: Some(SprfDiagPosition {
            line_lo: 1,
            col_lo: 4,
            line_hi: 1,
            col_hi: 7,
        }),
    };
    let json = serde_json::to_string(&d).unwrap();
    let parsed: SprfDiag = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.position, d.position);
    assert_eq!(parsed.uri, d.uri);
    assert_eq!(parsed.lo, d.lo);
    assert_eq!(parsed.hi, d.hi);
}
