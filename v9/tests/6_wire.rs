//! The generated router on a real localhost port, every route and every error model hit with a
//! fixed `Wire` as axum state; then the runtime's `Handle` as the state, its Diags answered with
//! the declared models.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use v9::_2_ir::Col::Text;
use v9::_2_ir::*;
use v9::_4_recipe::Recipe as SystemRecipe;
use v9::_8_runtime::_3_core::build;
use v9::_8_runtime::_4_handle::start;
use v9::_8_runtime::_5_run::BOUNDS;
use v9::_9_wire::http_auto::router;
use v9::_9_wire::models::cell::Cell;
use v9::_9_wire::models::col_type::ColType;
use v9::_9_wire::models::column::Column;
use v9::_9_wire::models::commit_refused::CommitRefused;
use v9::_9_wire::models::delta::Delta;
use v9::_9_wire::models::plan::Plan;
use v9::_9_wire::models::recipe::Recipe;
use v9::_9_wire::models::rel_kind::RelKind;
use v9::_9_wire::models::relation::Relation;
use v9::_9_wire::models::sql_rejected::SqlRejected;
use v9::_9_wire::models::table::Table;
use v9::_9_wire::ops_auto::{CommitArgs, OpError, OpResult, PlanArgs, QueryArgs, RecipeArgs, Wire};

struct Fixed;

impl Wire for Fixed {
    /// Echoes the changes; a change to `reach` is refused.
    fn commit(&self, args: &CommitArgs) -> OpResult<Delta> {
        if let Some(change) = args.changes.iter().find(|change| change.rel == "reach") {
            return Err(CommitRefused {
                diag: format!("refused {}", change.rel),
            }
            .into());
        }
        Ok(Delta {
            event: 7,
            changes: args.changes.clone(),
        })
    }
    fn query(&self, args: &QueryArgs) -> OpResult<Table> {
        match args.request.sql.as_str() {
            "select a, b, c, d from edge" => Ok(Table {
                columns: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                rows: vec![vec![
                    Some(Cell::text("x".into())),
                    Some(Cell::int(3)),
                    Some(Cell::real(2.5)),
                    None,
                ]],
            }),
            "boom" => Err(OpError::Internal("boom".into())),
            sql => Err(SqlRejected {
                diag: format!("unknown sql: {sql}"),
            }
            .into()),
        }
    }
    fn plan(&self, _args: &PlanArgs) -> OpResult<Plan> {
        Ok(Plan {
            relations: vec![
                Relation {
                    name: "edge".into(),
                    cols: vec![
                        Column {
                            name: "from".into(),
                            r#type: ColType::Text,
                        },
                        Column {
                            name: "weight".into(),
                            r#type: ColType::Int,
                        },
                    ],
                    kind: RelKind::Base,
                    service: None,
                },
                Relation {
                    name: "calls".into(),
                    cols: vec![Column {
                        name: "file".into(),
                        r#type: ColType::Text,
                    }],
                    kind: RelKind::Request,
                    service: Some("ryi".into()),
                },
            ],
        })
    }
    fn recipe(&self, _args: &RecipeArgs) -> OpResult<Recipe> {
        Ok(Recipe {
            canonical: "engine=dd,store=sqlite,wire=http".into(),
        })
    }
}

/// `(method, path, body)`; a `Raw` body goes out as-is with a JSON content type.
enum Body {
    None,
    Json(Value),
    Raw(&'static str),
}

/// Serves `router` on a free localhost port, sends every request in order, and returns each as
/// `{route, status, body}`.
async fn exchange(app: axum::Router, requests: Vec<(&str, &str, Body)>) -> Value {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut seen = Vec::new();
    for (method, path, body) in requests {
        let request = match method {
            "POST" => client.post(format!("{base}{path}")),
            _ => client.get(format!("{base}{path}")),
        };
        let request = match body {
            Body::None => request,
            Body::Json(body) => request.json(&body),
            Body::Raw(text) => request
                .header("content-type", "application/json")
                .body(text),
        };
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let value: Value = response.json().await.unwrap();
        seen.push(json!({ "route": format!("{method} {path}"), "status": status, "body": value }));
    }
    server.abort();
    Value::Array(seen)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_router_serves_every_route() {
    let seen = exchange(
        router(Arc::new(Fixed)),
        vec![
            (
                "POST",
                "/commit",
                Body::Json(json!([
                    { "rel": "edge", "cells": ["a", 2], "diff": 1 },
                    { "rel": "edge", "cells": ["b", -4.5, null], "diff": -1 },
                ])),
            ),
            (
                "POST",
                "/commit",
                Body::Json(json!([{ "rel": "reach", "cells": ["a"], "diff": 1 }])),
            ),
            (
                "POST",
                "/query",
                Body::Json(json!({ "sql": "select a, b, c, d from edge" })),
            ),
            (
                "POST",
                "/query",
                Body::Json(json!({ "sql": "drop table edge" })),
            ),
            ("POST", "/query", Body::Json(json!({ "sql": "boom" }))),
            ("GET", "/plan", Body::None),
            ("GET", "/recipe", Body::None),
            ("POST", "/commit", Body::Json(json!({ "rel": "edge" }))),
            ("POST", "/query", Body::Raw("not json")),
        ],
    )
    .await;

    assert_eq!(
        seen,
        json!([
            {
                "route": "POST /commit",
                "status": 200,
                "body": {
                    "event": 7,
                    "changes": [
                        { "rel": "edge", "cells": ["a", 2], "diff": 1 },
                        { "rel": "edge", "cells": ["b", -4.5, null], "diff": -1 },
                    ],
                },
            },
            {
                "route": "POST /commit",
                "status": 400,
                "body": { "diag": "refused reach" },
            },
            {
                "route": "POST /query",
                "status": 200,
                "body": {
                    "columns": ["a", "b", "c", "d"],
                    "rows": [["x", 3, 2.5, null]],
                },
            },
            {
                "route": "POST /query",
                "status": 400,
                "body": { "diag": "unknown sql: drop table edge" },
            },
            {
                "route": "POST /query",
                "status": 500,
                "body": { "error": "boom" },
            },
            {
                "route": "GET /plan",
                "status": 200,
                "body": {
                    "relations": [
                        {
                            "name": "edge",
                            "cols": [{ "name": "from", "type": "Text" }, { "name": "weight", "type": "Int" }],
                            "kind": "Base",
                            "service": null,
                        },
                        {
                            "name": "calls",
                            "cols": [{ "name": "file", "type": "Text" }],
                            "kind": "Request",
                            "service": "ryi",
                        },
                    ],
                },
            },
            {
                "route": "GET /recipe",
                "status": 200,
                "body": { "canonical": "engine=dd,store=sqlite,wire=http" },
            },
            {
                "route": "POST /commit",
                "status": 422,
                "body": {
                    "error": "Failed to deserialize the JSON body into the target type: invalid type: map, expected a sequence at line 1 column 0",
                },
            },
            {
                "route": "POST /query",
                "status": 422,
                "body": {
                    "error": "Failed to parse the request body as JSON: expected ident at line 1 column 2",
                },
            },
        ])
    );
}

/// The runtime's `Handle` behind the router: a Blob answer cell and SQL the store rejects are
/// 400 `SqlRejected`, a change the runtime does not take is 400 `CommitRefused`, a body that does
/// not decode is 422 `BodyRejected`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handle_answers_diags_with_the_declared_models() {
    let dir = std::env::temp_dir().join(format!("v9-6-wire-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("handle.db");
    let program = Program {
        rels: vec![
            rel("edge", [("from", Text), ("to", Text)]),
            derived("reach", [("from", Text), ("to", Text)]),
        ],
        rules: vec![rule(
            atom("reach", [v("a"), v("c")]),
            [pos("edge", [v("a"), v("c")])],
        )],
    };
    let handle = start(move || build(SystemRecipe::default(), program, &db, Vec::new(), BOUNDS))
        .expect("core");
    let seen = exchange(
        router(Arc::new(handle)),
        vec![
            (
                "POST",
                "/commit",
                Body::Json(json!([{ "rel": "edge", "cells": ["a", "b"], "diff": 1 }])),
            ),
            (
                "POST",
                "/commit",
                Body::Json(json!([{ "rel": "reach", "cells": ["a", "b"], "diff": 1 }])),
            ),
            (
                "POST",
                "/query",
                Body::Json(json!({ "sql": "SELECT * FROM reach_text" })),
            ),
            (
                "POST",
                "/query",
                Body::Json(json!({ "sql": "SELECT x'00' AS b" })),
            ),
            (
                "POST",
                "/query",
                Body::Json(json!({ "sql": "DELETE FROM edge" })),
            ),
            ("POST", "/commit", Body::Raw("[")),
        ],
    )
    .await;
    assert_eq!(
        seen,
        json!([
            {
                "route": "POST /commit",
                "status": 200,
                "body": {
                    "event": 2,
                    "changes": [
                        { "rel": "edge", "cells": ["a", "b"], "diff": 1 },
                        { "rel": "reach", "cells": ["a", "b"], "diff": 1 },
                    ],
                },
            },
            {
                "route": "POST /commit",
                "status": 400,
                "body": {
                    "diag": "Commit { change: 0, rel: \"reach\", problem: \"kind\", found: \"Derived\" }",
                },
            },
            {
                "route": "POST /query",
                "status": 200,
                "body": { "columns": ["from", "to"], "rows": [["a", "b"]] },
            },
            {
                "route": "POST /query",
                "status": 400,
                "body": { "diag": "Store(\"no wire form for Blob([0])\")" },
            },
            {
                "route": "POST /query",
                "status": 400,
                "body": { "diag": "Store(\"query is not a read-only select: DELETE FROM edge\")" },
            },
            {
                "route": "POST /commit",
                "status": 422,
                "body": {
                    "error": "Failed to parse the request body as JSON: EOF while parsing a list at line 1 column 1",
                },
            },
        ])
    );
}
