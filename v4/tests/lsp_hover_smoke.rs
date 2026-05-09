use v4::app::{
    build_in_process, LspHoverReq, LspOpenReq, SprfClient, SprfError,
};

#[tokio::test]
async fn lsp_hover_unknown_doc_errors() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let err = client.lsp_hover(LspHoverReq {
        uri: "file:///never-opened.sprf".into(),
        byte: 0,
    }).await.unwrap_err();

    assert!(
        matches!(err, SprfError::UnknownDoc(_)),
        "hover cache miss for unopened uri must surface as UnknownDoc, got {err:?}",
    );
}

#[tokio::test]
async fn lsp_hover_inside_sql_body_uses_dsl_provider() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "sql`SELECT input.__cursor_idx FROM input`";
    let body_lo = src.find('`').unwrap() + 1;
    let probe = body_lo + src[body_lo..].find("input").unwrap();

    client.lsp_open(LspOpenReq {
        uri: "file:///sql-hover.sprf".into(),
        text: src.into(),
        version: 1,
    }).await.unwrap();

    let hover = client.lsp_hover(LspHoverReq {
        uri: "file:///sql-hover.sprf".into(),
        byte: probe as u32,
    }).await.unwrap();

    assert_eq!(
        hover.contents.as_deref(),
        Some("current upstream cursor batch as a temp relation"),
    );
}

#[tokio::test]
async fn lsp_hover_outside_dsl_returns_empty() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "sql`SELECT input.value` > void";
    client.lsp_open(LspOpenReq {
        uri: "file:///outside-hover.sprf".into(),
        text: src.into(),
        version: 1,
    }).await.unwrap();

    let hover = client.lsp_hover(LspHoverReq {
        uri: "file:///outside-hover.sprf".into(),
        byte: src.find('>').unwrap() as u32,
    }).await.unwrap();

    assert_eq!(hover.contents, None);
}

#[tokio::test]
async fn lsp_hover_on_host_op_reports_cursor_flow_count() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "rule(:words, WORD?) { `alpha beta` > split(WORD?)` ` };";
    client.lsp_open(LspOpenReq {
        uri: "file:///cursor-flow-hover.sprf".into(),
        text: src.into(),
        version: 1,
    }).await.unwrap();

    let hover = client.lsp_hover(LspHoverReq {
        uri: "file:///cursor-flow-hover.sprf".into(),
        byte: src.find("split").unwrap() as u32,
    }).await.unwrap();

    assert_eq!(
        hover.contents.as_deref(),
        Some("split\n2 cursor(s)\nspan 37..52"),
    );
}
