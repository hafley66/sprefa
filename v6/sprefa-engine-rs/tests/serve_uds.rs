// The socket-file golden: the conformance fixture filter_map_is_a_level_rule
// compiled to Rust, booted on a temp socket, folded through POST /arrive, read
// back through GET /rel/{name}, and diffed against the fixture's oracle rows.
//
// dl6 (compile/dl_view/filter_map_is_a_level_rule.dl6):
//   doubled(Name, Out) <- reading(Name, Value), Value >= 10, Out := Value * 2.
// rx: arrivals$.pipe(concatMap(batch => driver.tick(batch))) folds the rel;
// reading$ is the arrival subject and doubled$ its filter+map projection, so a
// read is `latest` over the folded state.

use std::path::{Path, PathBuf};
use std::process::Command;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1::SendRequest;
use serde_json::{json, Value};
use sprefa_engine_rs::program::{run_boot, GenProgram};
use sprefa_engine_rs::serve::{bind, serve_on, ServeState};
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::ProgramJson;
use tokio_util::sync::CancellationToken;

mod emitted {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../prolog/compile/out/filter_map_is_a_level_rule.types.rs"
    ));
}

const FIXTURE: &str = "filter_map_is_a_level_rule";

fn engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// grade.pl's own generator, one fixture instead of the corpus.
fn compile_fixture(out_dir: &Path) -> GenProgram {
    let engine = engine_dir();
    let sweep = engine.join("../prolog/sweep.pl");
    let grade = engine.join("grade.pl");
    let fixtures = engine.join("../prolog/conformance/fixtures/operators.pl");
    let goal = format!(
        "once((sweep:read_all_fixtures('{}', Entries), member(entry({FIXTURE}, Term, Bindings), Entries), rust_grade:generate_one({FIXTURE}, Term, Bindings, '{}', user_error)))",
        fixtures.display(),
        out_dir.display()
    );
    let output = Command::new("swipl")
        .args(["-q", "-l"])
        .arg(&sweep)
        .args(["-l"])
        .arg(&grade)
        .args(["-g", &goal, "-g", "halt"])
        .output()
        .expect("run the Rust emitter for one fixture");
    assert!(
        output.status.success(),
        "emit failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let module_text =
        std::fs::read_to_string(out_dir.join(format!("{FIXTURE}.rs"))).expect("read emitted module");
    let start = module_text.find("r#\"").expect("raw string open") + 3;
    let end = module_text[start..].find("\"#;").expect("raw string close") + start;
    let program_json: ProgramJson =
        serde_json::from_str(&module_text[start..end]).expect("emitted program json");
    GenProgram::from_json(program_json)
}

fn oracle_final() -> Value {
    let path = engine_dir().join(format!(
        "../prolog/compile/out/{FIXTURE}.oracle.final.jsonl"
    ));
    let text = std::fs::read_to_string(path).expect("read oracle final rows");
    let document: Value = serde_json::from_str(text.trim()).expect("oracle final json");
    document["final"].clone()
}

struct Served {
    state: ServeState,
    socket: PathBuf,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
    _temp: tempfile::TempDir,
}

async fn boot_on_socket() -> Served {
    let temp = tempfile::tempdir().expect("temporary directory");
    let program = compile_fixture(temp.path());
    let seam = SqliteSeam::in_memory().expect("open seam");
    seam.run_ddl(&program.ddl).expect("run ddl");
    run_boot(&seam, &program.boot);
    let state = ServeState::spawn(program, seam);
    let socket = temp.path().join("program.sock");
    let listener = bind(&socket).expect("bind socket file");
    let cancel = CancellationToken::new();
    let task = {
        let state = state.clone();
        let socket = socket.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            serve_on(state, listener, &socket, cancel).await.expect("serve");
        })
    };
    Served {
        state,
        socket,
        cancel,
        task,
        _temp: temp,
    }
}

async fn client(socket: &Path) -> SendRequest<Full<Bytes>> {
    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .expect("connect socket file");
    let (sender, connection) = hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(
        stream,
    ))
    .await
    .expect("http handshake on the socket file");
    tokio::spawn(connection);
    sender
}

async fn call(
    sender: &mut SendRequest<Full<Bytes>>,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut builder = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "sprefa");
    let payload = match body {
        Some(document) => {
            builder = builder.header(hyper::header::CONTENT_TYPE, "application/json");
            Full::new(Bytes::from(document.to_string()))
        }
        None => Full::new(Bytes::new()),
    };
    let response = sender
        .send_request(builder.body(payload).expect("build request"))
        .await
        .expect("send request");
    let status = response.status().as_u16();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read response body")
        .to_bytes();
    let document: Value = serde_json::from_slice(&bytes).expect("json response body");
    (status, document)
}

// The response rows are objects keyed by column name; the oracle rows are
// arrays in column order, so the read folds back to the oracle's own shape.
fn rows_as_arrays(read: &Value) -> Vec<Value> {
    let columns: Vec<&str> = read["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .map(|column| column.as_str().expect("column name"))
        .collect();
    let mut rows: Vec<Value> = read["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| Value::Array(columns.iter().map(|column| row[*column].clone()).collect()))
        .collect();
    rows.sort_by_key(|row| row.to_string());
    rows
}

fn sorted(rows: &Value) -> Vec<Value> {
    let mut rows: Vec<Value> = rows.as_array().expect("oracle rows").clone();
    rows.sort_by_key(|row| row.to_string());
    rows
}

fn arrive(rel: &str, sign: &str, row: Value) -> Value {
    json!([{ "rel": rel, "sign": sign, "row": row }])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folded_program_answers_its_rels_over_a_socket_file() {
    let served = boot_on_socket().await;
    let mut sender = client(&served.socket).await;

    let (status, health) = call(&mut sender, "GET", "/health", None).await;
    assert_eq!(status, 200);
    assert_eq!(health["program"], json!(FIXTURE));

    let (status, first) = call(
        &mut sender,
        "POST",
        "/arrive",
        Some(arrive("reading", "add", json!(["net", 30]))),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        first,
        json!({"tick": 1, "deltas": {"doubled": {"add": [["net", 60]], "del": []},
                                     "reading": {"add": [["net", 30]], "del": []}}}),
        "the fold answers the tick line the ticklog door writes"
    );

    let (status, second) = call(
        &mut sender,
        "POST",
        "/arrive",
        Some(arrive("reading", "del", json!(["cpu", 12]))),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(second["tick"], json!(2));

    let oracle = oracle_final();
    for rel in ["reading", "doubled"] {
        let (status, read) = call(&mut sender, "GET", &format!("/rel/{rel}"), None).await;
        assert_eq!(status, 200);
        assert_eq!(
            rows_as_arrays(&read),
            sorted(&oracle[rel]),
            "GET /rel/{rel} must equal the fixture's oracle rows"
        );
    }

    let (_, reading) = call(&mut sender, "GET", "/rel/reading", None).await;
    let typed: Vec<emitted::Reading> =
        serde_json::from_value(reading["rows"].clone()).expect("rows type as the emitted Reading");
    assert_eq!(
        typed,
        vec![
            emitted::Reading {
                name: "disk".to_string(),
                value: 4
            },
            emitted::Reading {
                name: "net".to_string(),
                value: 30
            }
        ]
    );
    let (_, doubled) = call(&mut sender, "GET", "/rel/doubled", None).await;
    let typed: Vec<emitted::Doubled> =
        serde_json::from_value(doubled["rows"].clone()).expect("rows type as the emitted Doubled");
    assert_eq!(
        typed,
        vec![emitted::Doubled {
            name: "net".to_string(),
            out: 60
        }]
    );

    let (status, missing) = call(&mut sender, "GET", "/rel/nowhere", None).await;
    assert_eq!(status, 404);
    assert_eq!(missing["error"], json!("no rel named nowhere"));

    served.cancel.cancel();
    served.task.await.expect("server task");
    assert!(
        !served.socket.exists(),
        "graceful shutdown removes the socket file"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn two_clients_share_one_folded_state() {
    let served = boot_on_socket().await;
    let mut writer = client(&served.socket).await;
    let mut reader = client(&served.socket).await;

    let (status, _) = call(
        &mut writer,
        "POST",
        "/arrive",
        Some(arrive("reading", "add", json!(["net", 30]))),
    )
    .await;
    assert_eq!(status, 200);

    let (status, read) = call(&mut reader, "GET", "/rel/doubled", None).await;
    assert_eq!(status, 200);
    assert_eq!(
        rows_as_arrays(&read),
        vec![json!(["cpu", 24]), json!(["net", 60])],
        "the second client reads the first client's fold, boot rows included"
    );

    served.cancel.cancel();
    served.task.await.expect("server task");
}

// The engine thread is the concatMap: a read racing a fold lands before or
// after that whole batch, so the answer is the pre state or the post state.
#[tokio::test(flavor = "multi_thread")]
async fn a_read_during_a_fold_is_never_torn() {
    let served = boot_on_socket().await;
    let mut writer = client(&served.socket).await;
    let mut reader = client(&served.socket).await;

    let fold = call(
        &mut writer,
        "POST",
        "/arrive",
        Some(arrive("reading", "add", json!(["net", 30]))),
    );
    let read = call(&mut reader, "GET", "/rel/doubled", None);
    let ((fold_status, _), (read_status, read_body)) = tokio::join!(fold, read);
    assert_eq!(fold_status, 200);
    assert_eq!(read_status, 200);

    let before: Vec<Value> = vec![json!(["cpu", 24])];
    let after: Vec<Value> = vec![json!(["cpu", 24]), json!(["net", 60])];
    let rows = rows_as_arrays(&read_body);
    assert!(
        rows == before || rows == after,
        "a read answers the pre-tick or the post-tick state, never a half-folded one: {rows:?}"
    );

    let (_, settled) = call(&mut reader, "GET", "/rel/doubled", None).await;
    assert_eq!(rows_as_arrays(&settled), after);

    let _ = served.state.read_rel("doubled").await.expect("library read");
    served.cancel.cancel();
    served.task.await.expect("server task");
}
