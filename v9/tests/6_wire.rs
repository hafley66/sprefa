//! The generated router on a real localhost port, every route hit once with a fixed `Wire`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use v9::_9_wire::http_auto::router;
use v9::models::cell::Cell;
use v9::models::change::Change;
use v9::models::col_type::ColType;
use v9::models::column::Column;
use v9::models::delta::Delta;
use v9::models::plan::Plan;
use v9::models::query_request::QueryRequest;
use v9::models::recipe::Recipe;
use v9::models::rel_kind::RelKind;
use v9::models::relation::Relation;
use v9::models::table::Table;
use v9::ops::{install, Wire};
use v9::ops_auto::{OpError, OpResult};

struct Fixed;

impl Wire for Fixed {
    fn commit(&self, changes: &[Change]) -> OpResult<Delta> {
        Ok(Delta { event: 7, changes: changes.to_vec() })
    }
    fn query(&self, request: &QueryRequest) -> OpResult<Table> {
        if request.sql != "select a, b from edge" {
            return Err(OpError(format!("unknown sql: {}", request.sql)));
        }
        Ok(Table {
            columns: vec!["a".into(), "b".into()],
            rows: vec![vec![
                Cell { text: Some("x".into()), int: None },
                Cell { text: None, int: Some(3) },
            ]],
        })
    }
    fn plan(&self) -> OpResult<Plan> {
        Ok(Plan {
            relations: vec![
                Relation {
                    name: "edge".into(),
                    cols: vec![
                        Column { name: "from".into(), col_type: ColType::Text },
                        Column { name: "weight".into(), col_type: ColType::Int },
                    ],
                    kind: RelKind::Base,
                    service: None,
                },
                Relation {
                    name: "calls".into(),
                    cols: vec![Column { name: "file".into(), col_type: ColType::Text }],
                    kind: RelKind::Request,
                    service: Some("ryi".into()),
                },
            ],
        })
    }
    fn recipe(&self) -> OpResult<Recipe> {
        Ok(Recipe { canonical: "engine=dd,store=sqlite,wire=http".into() })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_router_serves_every_route() {
    install(Arc::new(Fixed)).expect("first install");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router()).await });
    let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap();

    let commit_body = json!([
        { "rel": "edge", "cells": [{ "text": "a" }, { "int": 2 }], "diff": 1 },
        { "rel": "edge", "cells": [{ "text": "b" }, { "int": -4 }], "diff": -1 },
    ]);
    let mut seen = Vec::new();
    for (method, path, body) in [
        ("POST", "/commit", Some(commit_body)),
        ("POST", "/query", Some(json!({ "sql": "select a, b from edge" }))),
        ("POST", "/query", Some(json!({ "sql": "drop table edge" }))),
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
                        { "rel": "edge", "cells": [{ "text": "a", "int": null }, { "text": null, "int": 2 }], "diff": 1 },
                        { "rel": "edge", "cells": [{ "text": "b", "int": null }, { "text": null, "int": -4 }], "diff": -1 },
                    ],
                },
            },
            {
                "route": "POST /query",
                "status": 200,
                "body": {
                    "columns": ["a", "b"],
                    "rows": [[{ "text": "x", "int": null }, { "text": null, "int": 3 }]],
                },
            },
            {
                "route": "POST /query",
                "status": 500,
                "body": { "error": "unknown sql: drop table edge" },
            },
            {
                "route": "GET /plan",
                "status": 200,
                "body": {
                    "relations": [
                        {
                            "name": "edge",
                            "cols": [{ "name": "from", "col_type": "Text" }, { "name": "weight", "col_type": "Int" }],
                            "kind": "Base",
                            "service": null,
                        },
                        {
                            "name": "calls",
                            "cols": [{ "name": "file", "col_type": "Text" }],
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
