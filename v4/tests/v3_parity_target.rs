use std::fs;
use std::sync::Arc;

use effect_runtime::v2::{
    table_dirty_key, FactStore, QueueBackend, SqliteFactStore, SqliteQueue, TABLE_DOMAIN,
};

use v4::app::{
    build_in_process, build_router, GetDiagsReq, GetFactTableReq, InProcessClient, LspOpenReq,
    RunReq, SprfClient, SprfDiag, SprfState, RunReport,
};

fn diag_lines(diags: &[SprfDiag]) -> String {
    diags
        .iter()
        .map(|d| format!("{}:{}:{}", d.severity, d.code, d.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn report_lines(report: &RunReport) -> String {
    let mut lines = Vec::new();
    lines.push("parse:".to_string());
    lines.push(diag_lines(&report.parse_diags));
    lines.push("walk:".to_string());
    lines.push(diag_lines(&report.walk_diags));
    lines.join("\n")
}

#[tokio::test]
async fn runtime_lsp_warn_publishes_diagnostics_for_open_buffer() {
    let root = tempfile::tempdir().unwrap();
    let (_state, client) = build_in_process(root.path().to_path_buf());

    let uri = "file:///v3-parity-runtime-diag.sprf".to_string();
    let src = r#"
        rule(:warns) {
          `alpha`
          > lsp_warn(:v3_parity)`runtime warning`
        };
    "#;

    client
        .lsp_open(LspOpenReq { uri: uri.clone(), text: src.to_string(), version: 1 })
        .await
        .unwrap();

    let diags = client.get_diags(GetDiagsReq { uri }).await.unwrap();
    assert!(
        diags.iter().any(|d| {
            d.severity == "warning"
                && d.code == "v3_parity"
                && d.message == "runtime warning"
        }),
        "runtime lsp_warn should surface through get_diags, got:\n{}",
        diag_lines(&diags)
    );
}

#[tokio::test]
async fn runtime_lsp_warn_bracket_term_focuses_capture_span() {
    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("lsp_term_focus.sprf");
    let src = r#"
        `before sh! after`
          > re`before (?P<NAME>sh!) after`
          > lsp_warn[NAME](:impure_shell)`shell ${NAME}`;
    "#;
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty()
            && report.walk_diags.is_empty()
            && report.runtime_diags.len() == 1,
        "focused lsp_warn should parse/lower/run cleanly:\n{}",
        report_lines(&report)
    );
    let diag = &report.runtime_diags[0];
    assert_eq!(diag.code, "impure_shell");
    assert_eq!(diag.message, "shell sh!");
    assert_eq!(diag.lo, Some(7));
    assert_eq!(diag.hi, Some(10));
}

#[tokio::test]
async fn write_file_backtick_path_writes_cursor_value() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("out.md");
    let sprf = root.path().join("write_file_backtick_path.sprf");
    let src = format!(
        "`hello from sprf` > write_file`{}`;",
        out.display()
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "write_file backtick path should parse/lower cleanly:\n{}",
        report_lines(&report)
    );

    let written = fs::read_to_string(&out).expect("write_file should create target file");
    assert_eq!(written, "hello from sprf");
}

#[tokio::test]
async fn lsp_open_does_not_execute_write_file() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("open-buffer-side-effect.txt");
    let uri = "file:///analysis-write-file.sprf".to_string();
    let src = format!("`from lsp_open` > write_file`{}`;", out.display());

    let (_state, client) = build_in_process(root.path().to_path_buf());
    client
        .lsp_open(LspOpenReq { uri: uri.clone(), text: src, version: 1 })
        .await
        .unwrap();

    assert!(
        !out.exists(),
        "lsp_open analysis mode should not execute write_file side effects",
    );
    let diags = client.get_diags(GetDiagsReq { uri }).await.unwrap();
    assert_eq!(diag_lines(&diags), "");
}

#[tokio::test]
async fn render_markdown_aggregate_writes_file() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("RECAP.md");
    let sprf = root.path().join("render_markdown_aggregate.sprf");
    let src = format!(
        r#"
        rule(:items, NAME?);

        `alpha` > term_bind(:NAME) > rule(:items, NAME: NAME);
        `beta`  > term_bind(:NAME) > rule(:items, NAME: NAME);

        items?(NAME?)
        > render_markdown`- ${{NAME}}`
        > write_file`{}`;
        "#,
        out.display()
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "markdown render/write should parse/lower cleanly:\n{}",
        report_lines(&report)
    );

    let first = fs::read_to_string(&out).expect("render should create markdown file");
    assert_eq!(first, "- alpha\n- beta\n");

    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();
    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "second render/write run should stay clean:\n{}",
        report_lines(&report)
    );
    let second = fs::read_to_string(&out).expect("render output should still exist");
    assert_eq!(second, first, "render/write should be idempotent");
}

#[tokio::test]
async fn render_markdown_interpolates_subpipe_links() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("TYPE_LINKS.md");
    let sprf = root.path().join("render_markdown_subpipe_links.sprf");
    let src = format!(
        r#"
        rule(:important_types, TYPE?, FILE?, LO?);

        `UserService`
          > term_bind(:TYPE)
          > `src/user.rs`
          > term_bind(:FILE)
          > `42`
          > term_bind(:LO)
          > important_types(TYPE, FILE, LO);

        `OrderStore`
          > term_bind(:TYPE)
          > `src/order.rs`
          > term_bind(:FILE)
          > `7`
          > term_bind(:LO)
          > important_types(TYPE, FILE, LO);

        important_types?(TYPE?, FILE?, LO?)
        > render_markdown`- ${{ TYPE > `**${{&.value}}**` }} at ${{ FILE > `[${{&.value}}:${{LO}}](${{&.value}}#L${{LO}})` }}
`
        > write_file`{}`;
        "#,
        out.display()
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty()
            && report.walk_diags.is_empty()
            && report.runtime_diags.is_empty(),
        "subpipe markdown render should stay clean:\n{}",
        report_lines(&report)
    );

    let got = fs::read_to_string(&out).expect("render should create markdown file");
    assert_eq!(
        got,
        "- **UserService** at [src/user.rs:42](src/user.rs#L42)\n- **OrderStore** at [src/order.rs:7](src/order.rs#L7)\n"
    );
}

#[tokio::test]
async fn render_markdown_replaces_comment_range_only() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("ARCH.md");
    let sprf = root.path().join("self_doc_range_replace.sprf");
    fs::write(&target, concat!(
        "# Architecture\n\n",
        "handwritten before\n",
        "<!-- sprf:self-doc:start -->\n",
        "old generated body\n",
        "<!-- sprf:self-doc:end -->\n",
        "handwritten after\n",
    )).unwrap();
    let src = format!(
        r#"`{}`
> read
> comment(`<!-- sprf:self-doc:start -->\n`, `\n<!-- sprf:self-doc:end -->`)
> render_markdown`## Generated
range-safe markdown`
> write_cursor(:replace);
"#,
        target.display(),
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty()
            && report.walk_diags.is_empty()
            && report.runtime_diags.is_empty(),
        "range render/write should stay clean:\n{}",
        report_lines(&report)
    );

    let written = fs::read_to_string(&target).unwrap();
    assert_eq!(written, concat!(
        "# Architecture\n\n",
        "handwritten before\n",
        "<!-- sprf:self-doc:start -->\n",
        "## Generated\n",
        "range-safe markdown\n",
        "\n",
        "<!-- sprf:self-doc:end -->\n",
        "handwritten after\n",
    ));

    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();
    assert!(
        report.parse_diags.is_empty()
            && report.walk_diags.is_empty()
            && report.runtime_diags.is_empty(),
        "second range render/write should stay clean:\n{}",
        report_lines(&report)
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), written);
}

#[tokio::test]
async fn render_dot_markdown_alias_writes_file() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("DOT_MARKDOWN.md");
    let sprf = root.path().join("render_dot_markdown.sprf");
    let src = format!(
        r#"`alpha`
> render.markdown`# ${{&.value}}`
> write_file`{}`;
"#,
        out.display()
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty()
            && report.walk_diags.is_empty()
            && report.runtime_diags.is_empty(),
        "render.markdown should parse/lower cleanly:\n{}",
        report_lines(&report)
    );

    assert_eq!(fs::read_to_string(&out).unwrap(), "# alpha\n");
}

#[tokio::test]
async fn app_run_keeps_queue_and_wakes_mounted_sql() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing.sprf");
    let hook = root.path().join("hook.sprf");

    fs::write(&missing, r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:missing_hooks, OP?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
                  AND frontend_hooks.FILE = ${FILE}
              )
            `
          > rule(:missing_hooks, OP: OP);
    "#).unwrap();
    fs::write(&hook, r#"
        rule(:frontend_hooks, OP?, FILE?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > rule(:frontend_hooks, OP: OP, FILE: FILE);
    "#).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    client.run(RunReq {
        path: missing,
        root: Some(root.path().to_path_buf()),
    }).await.unwrap();

    let table = client.get_fact_table(GetFactTableReq {
        name: "missing_hooks".into(),
        limit: None,
    }).await.unwrap();
    assert_eq!(table.total, 1);

    client.run(RunReq {
        path: hook,
        root: Some(root.path().to_path_buf()),
    }).await.unwrap();

    let table = client.get_fact_table(GetFactTableReq {
        name: "missing_hooks".into(),
        limit: None,
    }).await.unwrap();
    assert_eq!(table.total, 0);
}

#[tokio::test]
async fn app_can_use_sqlite_queue_for_mounted_sql_parks() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("queue.db");
    let missing = root.path().join("missing.sprf");

    fs::write(&missing, r#"
        rule(:frontend_hooks, OP?, FILE?);
        rule(:missing_hooks, OP?);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
                  AND frontend_hooks.FILE = ${FILE}
              )
            `
          > rule(:missing_hooks, OP: OP);
    "#).unwrap();

    {
        let state = Arc::new(SprfState::new_with_sqlite_queue(
            root.path().to_path_buf(),
            &db,
        ));
        let client = InProcessClient::new(build_router(state.clone()));
        client.run(RunReq {
            path: missing,
            root: Some(root.path().to_path_buf()),
        }).await.unwrap();

        assert_eq!(
            state.queue.depth(),
            1,
            "mounted sql should park one table-dirty continuation in sqlite queue",
        );
    }

    let queue: Arc<dyn QueueBackend<v4::Cursor>> =
        Arc::new(SqliteQueue::<v4::Cursor>::open_file(&db));
    assert_eq!(queue.depth(), 1);
    assert_eq!(
        queue.dispatch_park(TABLE_DOMAIN, Some(table_dirty_key("frontend_hooks"))),
        1,
        "sqlite-backed app queue should revive the mounted SQL park by table dirty key",
    );
}

#[tokio::test]
async fn app_can_use_sqlite_fact_store_for_rule_rows() {
    let root = tempfile::tempdir().unwrap();
    let fact_db = root.path().join("facts.db");
    let sprf = root.path().join("facts.sprf");

    fs::write(&sprf, r#"
        rule(:seen, WORD?);

        `alpha`
          > term_bind(:WORD)
          > rule(:seen, WORD: WORD);
    "#).unwrap();

    {
        let state = Arc::new(SprfState::new_with_sqlite_facts(
            root.path().to_path_buf(),
            &fact_db,
        ));
        let client = InProcessClient::new(build_router(state));
        client.run(RunReq {
            path: sprf,
            root: Some(root.path().to_path_buf()),
        }).await.unwrap();
    }

    let facts = SqliteFactStore::<v4::Cursor>::open_file(&fact_db).unwrap();
    let rows = facts.rows_of("seen");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("WORD"), Some("alpha"));
}

#[tokio::test]
async fn app_reopens_sqlite_backends_and_retracts_mounted_sql_outputs() {
    let root = tempfile::tempdir().unwrap();
    let fact_db = root.path().join("facts.db");
    let queue_db = root.path().join("queue.db");
    let sprf = root.path().join("mounted_reopen.sprf");

    fs::write(&sprf, r#"
        rule(:openapi_ops, OP?, SPEC?);
        rule(:frontend_hooks, OP?, FILE?);
        rule(:missing_hooks, OP?);

        `getUser`
          > term_bind(:OP)
          > `openapi.yaml`
          > term_bind(:SPEC)
          > openapi_ops(OP, SPEC);

        openapi_ops?(OP?, SPEC?)
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > missing_hooks(OP);
    "#).unwrap();

    {
        let state = Arc::new(SprfState::new_with_sqlite_backends(
            root.path().to_path_buf(),
            &fact_db,
            &queue_db,
        ));
        let client = InProcessClient::new(build_router(state));
        client.run(RunReq {
            path: sprf.clone(),
            root: Some(root.path().to_path_buf()),
        }).await.unwrap();
    }

    let facts = SqliteFactStore::<v4::Cursor>::open_file(&fact_db).unwrap();
    assert_eq!(facts.rows_of("missing_hooks").len(), 1);

    let queue: Arc<dyn QueueBackend<v4::Cursor>> =
        Arc::new(SqliteQueue::<v4::Cursor>::open_file(&queue_db));
    assert!(
        queue.depth() >= 1,
        "first run should leave mounted SQL continuations parked in SQLite",
    );

    fs::write(&sprf, r#"
        rule(:openapi_ops, OP?, SPEC?);
        rule(:frontend_hooks, OP?, FILE?);
        rule(:missing_hooks, OP?);

        `getUser`
          > term_bind(:OP)
          > `openapi.yaml`
          > term_bind(:SPEC)
          > openapi_ops(OP, SPEC);

        `getUser`
          > term_bind(:OP)
          > `src/hooks.ts`
          > term_bind(:FILE)
          > frontend_hooks(OP, FILE);

        openapi_ops?(OP?, SPEC?)
          > sql`
              SELECT input.__cursor_idx, input.OP
              FROM input
              WHERE NOT EXISTS (
                SELECT 1
                FROM frontend_hooks
                WHERE frontend_hooks.OP = ${OP}
              )
            `
          > missing_hooks(OP);
    "#).unwrap();

    {
        let state = Arc::new(SprfState::new_with_sqlite_backends(
            root.path().to_path_buf(),
            &fact_db,
            &queue_db,
        ));
        let client = InProcessClient::new(build_router(state));
        client.run(RunReq {
            path: sprf,
            root: Some(root.path().to_path_buf()),
        }).await.unwrap();
    }

    let facts = SqliteFactStore::<v4::Cursor>::open_file(&fact_db).unwrap();
    assert_eq!(
        facts.rows_of("missing_hooks").len(),
        0,
        "reopened SQLite facts should retain mounted output state and retract stale anti-join rows",
    );
}

#[tokio::test]
async fn app_write_file_wakes_mounted_read_pipeline() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data.txt");
    let reader = root.path().join("reader.sprf");
    let writer = root.path().join("writer.sprf");

    fs::write(&data, "alpha").unwrap();
    fs::write(&reader, format!(r#"
        rule(:contents, BODY?);

        `{}`
          > read
          > term_bind(:BODY)
          > rule(:contents, BODY: BODY);
    "#, data.display())).unwrap();
    fs::write(&writer, format!(r#"
        `bravo` > write_file`{}`;
    "#, data.display())).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    client.run(RunReq {
        path: reader,
        root: Some(root.path().to_path_buf()),
    }).await.unwrap();

    let table = client.get_fact_table(GetFactTableReq {
        name: "contents".into(),
        limit: None,
    }).await.unwrap();
    assert_eq!(table.total, 1);
    assert_eq!(
        table.rows[0].fields.iter()
            .find(|(name, _)| name == "BODY")
            .map(|(_, value)| value.as_str()),
        Some("alpha"),
    );

    client.run(RunReq {
        path: writer,
        root: Some(root.path().to_path_buf()),
    }).await.unwrap();

    let table = client.get_fact_table(GetFactTableReq {
        name: "contents".into(),
        limit: None,
    }).await.unwrap();
    let mut bodies: Vec<&str> = table.rows
        .iter()
        .filter_map(|row| {
            row.fields.iter()
                .find(|(name, _)| name == "BODY")
                .map(|(_, value)| value.as_str())
        })
        .collect();
    bodies.sort();
    assert_eq!(bodies, vec!["alpha", "bravo"]);
}
