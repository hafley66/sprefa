//! The wire hop (`examples/flow-services.dl`): a value crosses from a caller
//! in one service to the handler in another THROUGH the spec's operationId,
//! where no call edge exists.
//!
//! THE GATE: the client and server both define `createOrder` (generated stub +
//! handwritten handler), so single-def name resolution refuses the call edge —
//! the pair is unreachable through std/flow.dl alone. With the spec scanned,
//! the wire hop connects them end to end, forward (payload -> handler param)
//! and backward (handler return -> client result). The negative half deletes
//! the spec and demands the reach disappears.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = include_str!("../../examples/flow-services.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flow_services_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/client")).unwrap();
    fs::create_dir_all(dir.join("src/server")).unwrap();
    dir
}

fn run(dir: &Path) -> (i32, String, String) {
    let prog = format!("{PROG}\n? df_node(id, kind, var, fn, file, line).\n");
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", dir.join("db").to_str().unwrap()])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn sections(out: &str) -> Vec<String> {
    let mut secs: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.starts_with("? ") {
            secs.push(String::new());
        }
        if let Some(cur) = secs.last_mut() {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
    }
    secs
}

fn rows(sec: &str) -> Vec<Vec<String>> {
    // no tab filter: a single-column section (`? service_op`) has none.
    sec.lines()
        .filter(|l| l.starts_with("  ") && !l.trim_start().starts_with('('))
        .map(|l| l.trim_start().split('\t').map(|s| s.trim_end().to_string()).collect())
        .collect()
}

fn node_id(nodes: &[Vec<String>], kind: &str, var: &str, fn_frag: &str, out: &str) -> String {
    let hits: Vec<&Vec<String>> = nodes
        .iter()
        .filter(|r| r.len() >= 4 && r[1] == kind && r[2] == var && r[3].contains(fn_frag))
        .collect();
    assert_eq!(hits.len(), 1, "expected one {kind}/{var} in {fn_frag}, got {}:\n{out}", hits.len());
    hits[0][0].clone()
}

// NB: single literal — a `\n\` continuation would eat the YAML indentation.
const SPEC: &str = "openapi: 3.0.0\npaths:\n  /orders:\n    post:\n      operationId: createOrder\n";

const CLIENT: &str = "// GENERATED from openapi\n\
export function createOrder(payload: string): string { return payload; }\n\
export function place(order: string): void {\n\
    const resp = createOrder(order);\n\
    const shown = resp;\n\
}\n";

const SERVER: &str = "function createOrder(req: string): string {\n\
    const body = req;\n\
    return body;\n\
}\n";

fn write_services(d: &Path, with_spec: bool) {
    fs::write(d.join("src/client/api.ts"), CLIENT).unwrap();
    fs::write(d.join("src/server/handlers.ts"), SERVER).unwrap();
    if with_spec {
        fs::write(d.join("src/openapi.yaml"), SPEC).unwrap();
    }
}

#[test]
fn value_crosses_the_wire_through_the_spec() {
    let d = sandbox("pos");
    write_services(&d, true);
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");

    let secs = sections(&out);
    // order: service_op, wire_call, op_endpoint, service_reach, df_node
    assert!(secs.len() >= 5, "expected 5 query sections:\n{out}");
    assert!(
        rows(&secs[0]).iter().any(|r| r[0] == "createOrder"),
        "spec op scanned:\n{out}"
    );
    // both defs (stub + handler) are endpoints — the honest fan.
    let endpoints = rows(&secs[2]);
    assert!(
        endpoints.iter().filter(|r| r[0] == "createOrder").count() >= 2,
        "stub and handler both carry the op name:\n{out}"
    );

    let mut reach: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[3]) {
        reach.insert((r[0].clone(), r[1].clone()));
    }
    let nodes = rows(&secs[4]);
    let order = node_id(&nodes, "param", "order", "::place", &out);
    let req = node_id(&nodes, "param", "req", "handlers", &out);
    let body = node_id(&nodes, "let_bind", "body", "handlers", &out);
    let shown = node_id(&nodes, "let_bind", "shown", "::place", &out);

    // Forward across the wire: the client's payload lands in the handler.
    assert!(
        reach.contains(&(order.clone(), req.clone())),
        "order must reach the handler's req across the wire:\n{out}"
    );
    assert!(
        reach.contains(&(order.clone(), body.clone())),
        "order must reach the handler body binding:\n{out}"
    );
    // Backward: the handler's return reaches the client's consumer.
    assert!(
        reach.contains(&(req.clone(), shown.clone())),
        "req must flow back out to the client's shown:\n{out}"
    );
}

#[test]
fn no_spec_no_wire() {
    let d = sandbox("neg");
    write_services(&d, false);
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 5, "expected 5 query sections:\n{out}");
    assert!(rows(&secs[0]).is_empty(), "no spec -> no service_op rows:\n{out}");

    let mut reach: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[3]) {
        reach.insert((r[0].clone(), r[1].clone()));
    }
    let nodes = rows(&secs[4]);
    let order = node_id(&nodes, "param", "order", "::place", &out);
    let req = node_id(&nodes, "param", "req", "handlers", &out);
    // Two same-named defs: name resolution refuses, and without the spec
    // nothing else connects the services.
    assert!(
        !reach.contains(&(order, req)),
        "without the spec the wire must not exist:\n{out}"
    );
}
