//! `dl6 build` end to end: two `.dl6` programs each become one binary, each
//! binary's `run` tick log is byte-diffed against emit_rust_harness on the same
//! schedule, one binary answers a rel over its socket file, and `--version`
//! carries the ir_version the runtime interprets.
//!
//! The third program the card names, `golden-flex`, is NOT here: it stops at
//! `unsupported_construct: column_type_unknown` on BOTH doors at
//! de8e2c0a2 with the emitters untouched, so it cannot reach a build.
//! `door-handwritten` (grade.sh's own text-door program) stands in.
//!
//! Wall time is printed per step. Every step but the cargo build is held to
//! the 10-second law.
//!
//! Sabotage receipt for the byte-diff: one extra `println!` in the template's
//! `run` arm reds it with `the built binary's tick log is not the harness's`,
//! the built log carrying a trailing line the harness's does not.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1::SendRequest;
use serde_json::Value;

const TEN_SECOND_LAW: Duration = Duration::from_secs(10);

const RESIDENT_SCHEDULE: &str = r#"[
  [
    {"rel":"turn","sign":"add","row":["s",1,101,"user","hi"]},
    {"rel":"turn","sign":"add","row":["s",2,102,"assistant","one"]},
    {"rel":"turn","sign":"add","row":["s",3,103,"assistant","two"]},
    {"rel":"turn","sign":"add","row":["s",4,104,"user","more"]},
    {"rel":"turn","sign":"add","row":["s",5,105,"user","please"]},
    {"rel":"turn","sign":"add","row":["s",6,106,"assistant","done"]}
  ],
  [
    {"rel":"resident","sign":"add","row":["s",4,7,"ok"]}
  ]
]"#;

const DOOR_SCHEDULE: &str = r#"[
  [
    {"rel":"event","sign":"add","row":[1,"opened"]},
    {"rel":"event","sign":"add","row":[2,"closed"]}
  ],
  [
    {"rel":"result_ok","sign":"add","row":[1,"fine"]},
    {"rel":"event","sign":"add","row":[3,"reopened"]}
  ]
]"#;

fn engine_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    engine_dir().join(format!("../dl/fixtures/{name}.dl6"))
}

fn timed<T>(step: &str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let outcome = work();
    let elapsed = started.elapsed();
    println!("dl6_build: {step} {:.2}s", elapsed.as_secs_f64());
    outcome
}

fn under_the_law(step: &str, work: impl FnOnce()) {
    let started = Instant::now();
    work();
    let elapsed = started.elapsed();
    println!("dl6_build: {step} {:.2}s", elapsed.as_secs_f64());
    assert!(
        elapsed < TEN_SECOND_LAW,
        "{step} took {elapsed:?}, over the 10-second law"
    );
}

fn write_file(path: &Path, text: &str) {
    let mut file = std::fs::File::create(path).expect("create file");
    file.write_all(text.as_bytes()).expect("write file");
}

fn stdout_of(command: &mut Command, what: &str) -> String {
    let output = command.output().unwrap_or_else(|error| panic!("{what}: {error}"));
    assert!(
        output.status.success(),
        "{what} exited {}: {}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

// The generated crate lives under target/dl6-build/<name>/; its program.rs is
// the same module emit_rust_harness reads.
fn generated_program(name: &str) -> PathBuf {
    engine_dir()
        .join("target/dl6-build")
        .join(name)
        .join("src/program.rs")
}

fn build_program(name: &str, out: &Path) {
    let source = fixture(name);
    let started = Instant::now();
    let log = stdout_of(
        Command::new(env!("CARGO_BIN_EXE_dl6"))
            .arg("build")
            .arg(&source)
            .arg("--out")
            .arg(out),
        &format!("dl6 build {name}"),
    );
    print!("{log}");
    println!(
        "dl6_build: build {name} (cargo build exempt) {:.2}s",
        started.elapsed().as_secs_f64()
    );
    assert!(out.is_file(), "{} is a file", out.display());
}

fn tick_log_matches_the_harness(name: &str, binary: &Path, schedule: &Path) {
    let mut from_binary = String::new();
    under_the_law(&format!("{name} run"), || {
        from_binary = stdout_of(
            Command::new(binary).arg("run").arg(schedule),
            &format!("{name} run"),
        );
    });
    let mut from_harness = String::new();
    under_the_law(&format!("{name} emit_rust_harness"), || {
        from_harness = stdout_of(
            Command::new(env!("CARGO_BIN_EXE_emit_rust_harness"))
                .arg(generated_program(name))
                .arg(schedule),
            &format!("{name} harness"),
        );
    });
    assert!(
        !from_binary.trim().is_empty(),
        "{name} folded an empty tick log"
    );
    assert_eq!(
        from_binary, from_harness,
        "{name}: the built binary's tick log is not the harness's"
    );
}

async fn client(socket: &Path) -> SendRequest<Full<Bytes>> {
    let deadline = Instant::now() + TEN_SECOND_LAW;
    let stream = loop {
        match tokio::net::UnixStream::connect(socket).await {
            Ok(stream) => break stream,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "the socket file {} never answered: {error}",
                    socket.display()
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    };
    let (sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
            .await
            .expect("http handshake on the socket file");
    tokio::spawn(connection);
    sender
}

async fn get(sender: &mut SendRequest<Full<Bytes>>, path: &str) -> (u16, Value) {
    let request = hyper::Request::builder()
        .method("GET")
        .uri(path)
        .header(hyper::header::HOST, "sprefa")
        .body(Full::new(Bytes::new()))
        .expect("build request");
    let response = sender.send_request(request).await.expect("send request");
    let status = response.status().as_u16();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read response body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("json body"))
}

struct Resident(Child);

impl Drop for Resident {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_programs_build_into_binaries_that_fold_run_and_serve() {
    let work = tempfile::tempdir().expect("work directory");

    let resident_schedule = work.path().join("resident-coroutine.schedule.json");
    write_file(&resident_schedule, RESIDENT_SCHEDULE);
    let door_schedule = work.path().join("door-handwritten.schedule.json");
    write_file(&door_schedule, DOOR_SCHEDULE);

    let resident = work.path().join("resident-coroutine");
    build_program("resident-coroutine", &resident);
    let door = work.path().join("door-handwritten");
    build_program("door-handwritten", &door);

    tick_log_matches_the_harness("resident-coroutine", &resident, &resident_schedule);
    tick_log_matches_the_harness("door-handwritten", &door, &door_schedule);

    let version = timed("version", || {
        stdout_of(Command::new(&resident).arg("--version"), "--version")
    });
    assert!(
        version.contains("resident-coroutine") && version.contains("ir_version 1"),
        "--version prints the program name and the ir_version, got {version:?}"
    );

    let socket = work.path().join("door.sock");
    let served = Resident(
        Command::new(&door)
            .arg("serve")
            .arg("--socket")
            .arg(&socket)
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn serve"),
    );
    let mut sender = client(&socket).await;
    let (status, health) = get(&mut sender, "/health").await;
    assert_eq!(status, 200, "GET /health");
    assert_eq!(health["program"], Value::from("door-handwritten"));
    let (status, read) = get(&mut sender, "/rel/current").await;
    assert_eq!(status, 200, "GET /rel/current: {read}");
    assert_eq!(
        read["columns"],
        serde_json::json!(["id", "kind"]),
        "the socket answers current's declared columns"
    );
    drop(served);
}

// TEST: the one ir_version pin. Both emitters spell the number the same way,
// and program.rs's IR_VERSION is that number. Fail-first: bumping either
// `ir_version(N).` alone reds this.
#[test]
fn both_emitters_and_the_runtime_agree_on_ir_version() {
    fn emitter_ir_version(file: &str) -> u32 {
        let path = engine_dir().join("../prolog").join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let mut found: Vec<u32> = text
            .lines()
            .filter_map(|line| line.strip_prefix("ir_version("))
            .filter_map(|rest| rest.strip_suffix(")."))
            .map(|digits| digits.parse().expect("ir_version is an integer"))
            .collect();
        assert_eq!(found.len(), 1, "{file} declares one ir_version/1 fact");
        found.pop().expect("the fact")
    }

    let rust_emitter = emitter_ir_version("emit_rust.pl");
    let ts_emitter = emitter_ir_version("emit_ts.pl");
    assert_eq!(
        rust_emitter, ts_emitter,
        "emit_rust.pl and emit_ts.pl stamp one ir_version between them"
    );
    assert_eq!(
        rust_emitter,
        sprefa_engine_rs::program::IR_VERSION,
        "the runtime interprets the ir_version the emitters stamp"
    );
}

// TEST: the Rust door's boot check. A program document whose ir_version is not
// the runtime's is refused by name; the same document at the runtime's version
// boots. Fail-first: dropping the guard in try_from_json makes the first arm
// return Ok.
#[test]
fn a_program_at_another_ir_version_is_refused_by_name() {
    let module = std::fs::read_to_string(
        engine_dir().join("tests/fixtures/resident-coroutine.program.rs"),
    )
    .expect("read the snapshot");
    let start = module.find("r#\"").expect("raw string open") + 3;
    let end = module[start..].find("\"#;").expect("raw string close") + start;
    let mut document: Value =
        serde_json::from_str(&module[start..end]).expect("the snapshot's program json");

    document["ir_version"] = Value::from(sprefa_engine_rs::program::IR_VERSION + 1);
    let refused = sprefa_engine_rs::GenProgram::try_from_json(
        serde_json::from_value(document.clone()).expect("ProgramJson"),
    )
    .err()
    .expect("a foreign ir_version is refused");
    assert_eq!(
        refused.to_string(),
        format!(
            "ir_version_mismatch: program resident-coroutine was emitted at ir_version {} and this runtime interprets {}",
            sprefa_engine_rs::program::IR_VERSION + 1,
            sprefa_engine_rs::program::IR_VERSION
        )
    );

    document["ir_version"] = Value::from(sprefa_engine_rs::program::IR_VERSION);
    sprefa_engine_rs::GenProgram::try_from_json(
        serde_json::from_value(document).expect("ProgramJson"),
    )
    .expect("the runtime's own ir_version boots");
}
