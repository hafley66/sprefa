use v4::app::{build_in_process, GetDiagsReq, LspHoverReq, LspOpenReq, SprfClient, SprfError};

#[tokio::test]
async fn lsp_hover_unknown_doc_errors() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let err = client
        .lsp_hover(LspHoverReq {
            uri: "file:///never-opened.sprf".into(),
            byte: 0,
        })
        .await
        .unwrap_err();

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

    client
        .lsp_open(LspOpenReq {
            uri: "file:///sql-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///sql-hover.sprf".into(),
            byte: probe as u32,
        })
        .await
        .unwrap();

    assert_eq!(
        hover.contents.as_deref(),
        Some("current upstream cursor batch as a temp relation"),
    );
}

#[tokio::test]
async fn lsp_hover_inside_render_markdown_body_uses_markdown_provider() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "render_markdown`## ${TITLE}\n| A | B |`";
    let body_lo = src.find('`').unwrap() + 1;
    let probe = body_lo + src[body_lo..].find("${TITLE}").unwrap() + 3;

    client
        .lsp_open(LspOpenReq {
            uri: "file:///markdown-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///markdown-hover.sprf".into(),
            byte: probe as u32,
        })
        .await
        .unwrap();

    assert_eq!(
        hover.contents.as_deref(),
        Some("render interpolation\nreads `${TITLE}` from the cursor terms or focal fields"),
    );
}

#[tokio::test]
async fn lsp_hover_inside_render_dot_markdown_body_uses_markdown_provider() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "render.markdown`## ${TITLE}\n| A | B |`";
    let body_lo = src.find('`').unwrap() + 1;
    let probe = body_lo + src[body_lo..].find("${TITLE}").unwrap() + 3;

    client
        .lsp_open(LspOpenReq {
            uri: "file:///markdown-dot-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///markdown-dot-hover.sprf".into(),
            byte: probe as u32,
        })
        .await
        .unwrap();

    assert_eq!(
        hover.contents.as_deref(),
        Some("render interpolation\nreads `${TITLE}` from the cursor terms or focal fields"),
    );
}

#[tokio::test]
async fn lsp_hover_inside_path_body_reports_checked_path_status() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("exists.txt"), b"hello").unwrap();
    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let src = "path`exists.txt`;";
    let body_lo = src.find('`').unwrap() + 1;
    let probe = body_lo + src[body_lo..].find("exists").unwrap();

    client
        .lsp_open(LspOpenReq {
            uri: "file:///path-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///path-hover.sprf".into(),
            byte: probe as u32,
        })
        .await
        .unwrap();

    let expected = format!("path\nfile\n{}", tmp.path().join("exists.txt").display());
    assert_eq!(
        hover.contents.as_deref(),
        Some(expected.as_str()),
    );
}

#[tokio::test]
async fn lsp_path_missing_literal_emits_runtime_diag_with_span() {
    let tmp = tempfile::tempdir().unwrap();
    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let src = "path`missing.txt`;";
    client
        .lsp_open(LspOpenReq {
            uri: "file:///path-missing.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let diags = client
        .get_diags(GetDiagsReq {
            uri: "file:///path-missing.sprf".into(),
        })
        .await
        .unwrap();

    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, "path/missing");
    assert_eq!(diags[0].severity, "error");
    assert_eq!(diags[0].lo, Some(0));
    assert_eq!(diags[0].hi, Some((src.len() - 1) as u32));
    assert!(
        diags[0].message.contains("missing.txt"),
        "diag should include checked path, got {:?}",
        diags[0].message,
    );
}

#[tokio::test]
async fn lsp_hover_outside_dsl_returns_empty() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "sql`SELECT input.value` > void";
    client
        .lsp_open(LspOpenReq {
            uri: "file:///outside-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///outside-hover.sprf".into(),
            byte: src.find('>').unwrap() as u32,
        })
        .await
        .unwrap();

    assert_eq!(hover.contents, None);
}

#[tokio::test]
async fn lsp_hover_on_host_op_reports_cursor_flow_count() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "rule(:words, WORD?) { `alpha beta` > split(WORD?)` ` };";
    client
        .lsp_open(LspOpenReq {
            uri: "file:///cursor-flow-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///cursor-flow-hover.sprf".into(),
            byte: src.find("split").unwrap() as u32,
        })
        .await
        .unwrap();

    assert_eq!(
        hover.contents.as_deref(),
        Some("`split`\ncursors: 2\nspan: 37..52\nvalue: `alpha`\nterms: `WORD=alpha`"),
    );
}

#[tokio::test]
async fn lsp_hover_op_publishes_runtime_hover_at_term_focus() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "`before sh! after` > re`before (?P<NAME>sh!) after` > lsp.hover[NAME]`custom hover ${NAME}`;";
    client
        .lsp_open(LspOpenReq {
            uri: "file:///runtime-hover.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///runtime-hover.sprf".into(),
            byte: 8,
        })
        .await
        .unwrap();

    assert_eq!(hover.contents.as_deref(), Some("custom hover sh!"),);

    let diags = client
        .get_diags(GetDiagsReq {
            uri: "file:///runtime-hover.sprf".into(),
        })
        .await
        .unwrap();
    assert!(
        diags.iter().all(|d| d.code != "sprf/hover"),
        "runtime hover events should not leak as diagnostics: {diags:?}",
    );
}

#[tokio::test]
async fn lsp_hover_underscore_alias_publishes_runtime_hover() {
    let (_state, client) = build_in_process(std::env::temp_dir());

    let src = "`before sh! after` > re`before (?P<NAME>sh!) after` > lsp_hover[NAME]`alias hover ${NAME}`;";
    client
        .lsp_open(LspOpenReq {
            uri: "file:///runtime-hover-alias.sprf".into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let hover = client
        .lsp_hover(LspHoverReq {
            uri: "file:///runtime-hover-alias.sprf".into(),
            byte: 8,
        })
        .await
        .unwrap();

    assert_eq!(hover.contents.as_deref(), Some("alias hover sh!"),);
}
