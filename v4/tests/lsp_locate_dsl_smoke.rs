//! Boundary test: hover/completion lookup must consult the cached
//! `DocState.program` populated at `lsp_open` / `lsp_change`, not
//! re-parse the text on every call.
//!
//! Drives the in-process axum router via `InProcessClient`; never
//! touches `host_parse` directly. If `lsp_locate_dsl` exists and reads
//! the cache it passes; if anyone replaces it with a re-parse path the
//! `UnknownDoc` arm still passes (it's the cached map that's missing
//! the uri), so the second arm — a real backtick-body lookup against
//! an opened doc — is what catches re-parse regressions.
//!
//! Failing-first proof: this file does not compile until
//! `LspLocateDslReq` / `Resp` and the `lsp_locate_dsl` RPC exist.

use v4::app::{build_in_process, LspLocateDslReq, LspOpenReq, SprfClient, SprfError};

#[tokio::test]
async fn locate_dsl_unknown_doc_errors() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let err = client
        .lsp_locate_dsl(LspLocateDslReq {
            uri: "file:///never-opened.sprf".into(),
            byte: 0,
        })
        .await
        .unwrap_err();

    assert!(
        matches!(err, SprfError::UnknownDoc(_)),
        "cache miss for unopened uri must surface as UnknownDoc, got {err:?}",
    );
}

#[tokio::test]
async fn locate_dsl_inside_re_body_uses_cache() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    // Two ops on one pipe; cursor lands inside the `re` backtick body.
    // After lsp_open, DocState.program holds this pipe; lsp_locate_dsl
    // must walk it without re-parsing.
    let src = "re`hello world` > void";
    let body_lo = src.find('`').unwrap() + 1;
    let body_hi = src.rfind('`').unwrap();
    let probe = body_lo + 2; // somewhere inside "hello"

    client
        .lsp_open(LspOpenReq {
            uri: "file:///probe.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hit = client
        .lsp_locate_dsl(LspLocateDslReq {
            uri: "file:///probe.sprf".into(),
            byte: probe as u32,
        })
        .await
        .unwrap();

    assert_eq!(hit.op_name.as_deref(), Some("re"));
    assert_eq!(hit.body_off, body_lo as u32);
    assert_eq!(hit.body_byte, (probe - body_lo) as u32);
    let raw = hit.body_raw.expect("body_raw set when op_name is set");
    assert_eq!(raw, &src[body_lo..body_hi]);
}

#[tokio::test]
async fn locate_dsl_outside_any_body_returns_none() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "re`xy` > void";
    client
        .lsp_open(LspOpenReq {
            uri: "file:///none.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    // Position on the `>` host token — outside every dsl body.
    let probe = src.find('>').unwrap() as u32;
    let hit = client
        .lsp_locate_dsl(LspLocateDslReq {
            uri: "file:///none.sprf".into(),
            byte: probe,
        })
        .await
        .unwrap();

    assert!(hit.op_name.is_none());
    assert!(hit.body_raw.is_none());
}
