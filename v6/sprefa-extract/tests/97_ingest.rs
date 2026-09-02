//! `--ingest`: the reverse door, and the relation registry it validates against.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `d816c6ce5`
//! `--ingest` is an unknown flag, so clap exits 2 and every case below sees an
//! empty stdout and a usage dump on stderr. `schema_prints_every_registry_row`
//! fails to compile there: `sprefa_extract::tsi::REGISTRY` does not exist.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use sprefa_extract::tsi::{Arg, Method, TsiSink, REGISTRY};

const FIXTURE: &str = "tests/fixtures/tsi/foreign_probe.jsonl";

/// The one witnessed producer stream on this tree, which the door must accept
/// as readily as a hand-written foreign one.
const TS_FIXTURE: &str = "tests/fixtures/resolve/0_caller.ts";

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Run {
    fn lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }

    fn rows(&self) -> Vec<Value> {
        self.stdout
            .lines()
            .map(|line| serde_json::from_str(line).expect("an emitted row is JSON"))
            .collect()
    }

    fn facts(&self) -> Vec<Value> {
        self.rows()
            .into_iter()
            .filter(|row| row["record"] == "fact")
            .collect()
    }
}

/// The stream arrives on stdin, so a case mutates the fixture text without
/// writing a file.
fn ingest(stream: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--ingest", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("extract binary runs");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(stream.as_bytes())
        .expect("the stream fits the pipe");
    let output = child.wait_with_output().expect("extract exits");
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}

fn fixture() -> Vec<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    std::fs::read_to_string(path)
        .expect("the foreign fixture is readable")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Line numbers are 1-based on the wire, so a case names the line a reader
/// would count to.
fn with_line(number: usize, replacement: &str) -> String {
    let mut lines = fixture();
    lines[number - 1] = replacement.to_string();
    lines.join("\n")
}

fn arg_key(arg: &Value) -> (u8, i64, String) {
    if let Some(id) = arg.get("id") {
        (0, id.as_i64().expect("an id is a number"), String::new())
    } else if let Some(span) = arg.get("span") {
        (1, span[1].as_i64().unwrap_or(0), span[0].to_string())
    } else if let Some(text) = arg.get("text") {
        (2, 0, text.to_string())
    } else if let Some(int) = arg.get("int") {
        (3, int.as_i64().expect("an int is a number"), String::new())
    } else if let Some(atom) = arg.get("atom") {
        (4, 0, atom.to_string())
    } else {
        panic!("argument is none of the five kinds: {arg}")
    }
}

/// The canonical fact order the door numbers ids in: relation, then arguments,
/// with ids compared as numbers. Duplicated here on purpose, so the test states
/// the rule rather than reading it back out of the code under test.
fn canonical(facts: &mut [Value]) {
    facts.sort_by_key(|fact| {
        (
            fact["relation"]
                .as_str()
                .expect("a relation is a name")
                .to_string(),
            fact["args"]
                .as_array()
                .expect("args is a list")
                .iter()
                .map(arg_key)
                .collect::<Vec<_>>(),
        )
    });
}

fn ids_of(fact: &Value) -> Vec<u32> {
    fact["args"]
        .as_array()
        .expect("args is a list")
        .iter()
        .filter_map(|arg| arg.get("id").and_then(Value::as_u64).map(|id| id as u32))
        .collect()
}

/// Criterion 3: a foreign stream is accepted and every row it carried is
/// claimed by the door.
#[test]
fn foreign_stream_is_accepted() {
    let run = ingest(&fixture().join("\n"));
    assert!(run.ok, "stderr: {}", run.stderr);
    assert_eq!(
        run.lines()[0],
        r#"{"record":"protocol","version":1}"#,
        "the protocol row is not first"
    );
    let rows = run.rows();
    let foreign: Vec<u64> = rows
        .iter()
        .filter(|row| row["record"] == "witness" && row["method"] == "foreign")
        .map(|row| row["fact"].as_u64().expect("a witness names a fact"))
        .collect();
    let facts = run.facts();
    assert_eq!(facts.len(), 7, "the fixture carries seven fact rows");
    for fact in &facts {
        let ordinal = fact["fact"].as_u64().expect("a fact carries an ordinal");
        assert!(
            foreign.contains(&ordinal),
            "fact {ordinal} gained no foreign witness: {fact}"
        );
    }
}

/// Step 1, the arity half. The stop names the relation and the line.
#[test]
fn bad_arity_is_named() {
    let cut = r#"{"record":"fact","fact":4,"relation":"tsi.edge","args":[{"id":61},{"id":40},{"text":"id"},{"id":19}]}"#;
    let run = ingest(&with_line(7, cut));
    assert!(!run.ok, "a four-argument tsi.edge was accepted");
    assert!(run.stderr.contains("tsi.edge"), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("line 7"), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("arity 4"), "stderr: {}", run.stderr);
}

/// Step 1, the kind half. `position` is an `int` on the wire and a `text` that
/// spells a number is still the wrong shape (the no-coercions decision).
#[test]
fn bad_kind_names_the_position() {
    let coerced = r#"{"record":"fact","fact":4,"relation":"tsi.edge","args":[{"id":61},{"id":40},{"text":"id"},{"id":19},{"text":"0"}]}"#;
    let run = ingest(&with_line(7, coerced));
    assert!(!run.ok, "a text position was accepted");
    assert!(run.stderr.contains("position 4"), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("int"), "stderr: {}", run.stderr);
}

/// A relation the registry does not carry is a named stop, never a row the
/// door passes through unvalidated.
#[test]
fn unknown_relation_is_a_named_stop() {
    let unknown = r#"{"record":"fact","fact":6,"relation":"tsi.frobnicate","args":[{"id":61}]}"#;
    let run = ingest(&with_line(9, unknown));
    assert!(!run.ok, "tsi.frobnicate was accepted");
    assert!(
        run.stderr.contains("not in registry"),
        "stderr: {}",
        run.stderr
    );
    assert!(
        run.stderr.contains("tsi.frobnicate"),
        "stderr: {}",
        run.stderr
    );
}

/// Step 2. An id no row declares is a hole in the graph, and the door names it
/// rather than emitting a stream whose targets point nowhere.
#[test]
fn dangling_id_is_named() {
    let dangling = r#"{"record":"fact","fact":4,"relation":"tsi.edge","args":[{"id":61},{"id":40},{"text":"id"},{"id":9},{"int":0}]}"#;
    let run = ingest(&with_line(7, dangling));
    assert!(!run.ok, "an undeclared target id was accepted");
    assert!(run.stderr.contains("id 9"), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("line 7"), "stderr: {}", run.stderr);
}

/// A recursive type closes through ids, so the closure check is one pass and a
/// self-referencing edge terminates. The wall cap is the 10-second law.
#[test]
fn a_cycle_is_one_pass() {
    let cyclic = r#"{"record":"fact","fact":4,"relation":"tsi.edge","args":[{"id":61},{"id":40},{"text":"id"},{"id":40},{"int":0}]}"#;
    let started = Instant::now();
    let run = ingest(&with_line(7, cyclic));
    let elapsed = started.elapsed();
    assert!(run.ok, "stderr: {}", run.stderr);
    assert!(
        elapsed < Duration::from_secs(10),
        "a self-referencing edge took {elapsed:?}"
    );
}

/// Step 3. `complete` means absence from the relation is meaningful, so a
/// complete claim over an empty relation is a producer defect.
#[test]
fn complete_coverage_with_no_row_is_named() {
    let mut lines = fixture();
    lines.push(
        r#"{"record":"coverage","run":0,"relation":"tsi.sum","coverage":"complete"}"#.to_string(),
    );
    let run = ingest(&lines.join("\n"));
    assert!(!run.ok, "an empty complete relation was accepted");
    assert!(run.stderr.contains("tsi.sum"), "stderr: {}", run.stderr);
    assert!(run.stderr.contains("run 0"), "stderr: {}", run.stderr);
}

/// The steady state the whole canonicalization exists to have: the door's own
/// output is already at the fixpoint, so re-reading it changes no byte.
#[test]
fn ingest_is_idempotent() {
    let once = ingest(&fixture().join("\n"));
    assert!(once.ok, "stderr: {}", once.stderr);
    let twice = ingest(&once.stdout);
    assert!(twice.ok, "stderr: {}", twice.stderr);
    assert_eq!(once.stdout, twice.stdout);
}

/// Step 4. The fixture's ids are 40, 7, 19, 61 and 55; the door replaces them
/// with 0..n in first-appearance order over the canonically sorted fact rows,
/// which is the numbering identity rule 5 keys on.
#[test]
fn ids_are_renumbered_from_zero() {
    let run = ingest(&fixture().join("\n"));
    assert!(run.ok, "stderr: {}", run.stderr);
    for old in ["\"id\":40", "\"id\":19", "\"id\":61", "\"id\":55"] {
        assert!(
            !run.stdout.contains(old),
            "a producer id survived: {old}\n{}",
            run.stdout
        );
    }
    let mut facts = run.facts();
    canonical(&mut facts);
    let mut next = 0u32;
    for fact in &facts {
        for id in ids_of(fact) {
            assert!(
                id <= next,
                "id {id} appears before {next} was minted: {fact}"
            );
            if id == next {
                next += 1;
            }
        }
    }
    assert_eq!(next, 5, "the fixture declares five ids");
}

/// `--schema` is the contract a foreign producer reads, and the registry half
/// of it is printed from `REGISTRY` rather than re-typed.
#[test]
fn schema_prints_every_registry_row() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("--schema")
        .output()
        .expect("extract binary runs");
    let schema = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let rows: Vec<&str> = schema
        .lines()
        .filter(|line| line.trim_start().starts_with("relation="))
        .collect();
    assert_eq!(rows.len(), REGISTRY.len());
    for relation in REGISTRY {
        let kinds: Vec<&str> = relation.args.iter().map(|kind| kind.word()).collect();
        let expected = format!("  relation={} args=[{}]", relation.name, kinds.join(","));
        assert!(rows.contains(&expected.as_str()), "missing `{expected}`");
    }
}

/// The two doors meet: what `--witness` writes, `--ingest` reads back. The
/// stream carries no `record=fact` row yet, so this pins the envelope path.
#[test]
fn a_witnessed_extract_stream_ingests() {
    let produced = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--witness", "--family", "type", TS_FIXTURE])
        .output()
        .expect("extract binary runs");
    assert!(produced.status.success());
    let stream = String::from_utf8(produced.stdout).expect("stdout is UTF-8");
    assert!(!stream.is_empty(), "the producer emitted nothing");
    let run = ingest(&stream);
    assert!(run.ok, "stderr: {}", run.stderr);
    assert_eq!(run.lines()[0], r#"{"record":"protocol","version":1}"#);
}

/// The write side of the same registry: an adapter mints ids and facts through
/// the sink, and every fact leaves with its own witness.
#[test]
fn sink_numbers_facts_and_witnesses_together() {
    let mut sink = TsiSink::new(3, Method::CheckerWalk);
    let user = sink.fresh_id();
    let number = sink.fresh_id();
    assert_eq!((user, number), (0, 1));
    assert_eq!(sink.fact("tsi.type", vec![Arg::Id(user)]), 0);
    assert_eq!(sink.fact("tsi.product", vec![Arg::Id(user)]), 1);
    sink.complete("tsi.type");
    let rows: Vec<Value> = sink
        .rows()
        .iter()
        .map(|row| serde_json::to_value(row).expect("a flat fact serializes"))
        .collect();
    let records: Vec<&str> = rows
        .iter()
        .map(|row| row["record"].as_str().expect("a row is tagged"))
        .collect();
    assert_eq!(
        records,
        ["fact", "fact", "witness", "witness", "coverage"],
        "rows: {rows:?}"
    );
    for witness in rows.iter().filter(|row| row["record"] == "witness") {
        assert_eq!(witness["run"], 3);
        assert_eq!(witness["method"], "checker_walk");
    }
    assert_eq!(rows[4]["coverage"], "complete");
}

/// The sink's registry check is a `debug_assert!`, so an adapter that spells a
/// relation wrong fails its own tests instead of shipping a bad row.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "arity 1, the registry says 5")]
fn sink_stops_on_a_wrong_arity() {
    TsiSink::new(0, Method::CheckerWalk).fact("tsi.edge", vec![Arg::Id(0)]);
}
