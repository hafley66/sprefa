//! The generated router on a real localhost port, every route and every error model hit with a
//! fixed `Wire` as axum state.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_router_serves_every_route() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router(Arc::new(Fixed))).await });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let mut seen = Vec::new();
    for (method, path, body) in [
        (
            "POST",
            "/commit",
            Some(json!([
                { "rel": "edge", "cells": ["a", 2], "diff": 1 },
                { "rel": "edge", "cells": ["b", -4.5, null], "diff": -1 },
            ])),
        ),
        (
            "POST",
            "/commit",
            Some(json!([{ "rel": "reach", "cells": ["a"], "diff": 1 }])),
        ),
        (
            "POST",
            "/query",
            Some(json!({ "sql": "select a, b, c, d from edge" })),
        ),
        ("POST", "/query", Some(json!({ "sql": "drop table edge" }))),
        ("POST", "/query", Some(json!({ "sql": "boom" }))),
        ("GET", "/plan", None),
        ("GET", "/recipe", None),
    ] {
        let request = match method {
            "POST" => client.post(format!("{base}{path}")),
            _ => client.get(format!("{base}{path}")),
        };
        let request = match body {
            Some(body) => request.json(&body),
            None => request,
        };
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let value: Value = response.json().await.unwrap();
        seen.push(json!({ "route": format!("{method} {path}"), "status": status, "body": value }));
    }
    server.abort();

    assert_eq!(
        Value::Array(seen),
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
        ])
    );
}
