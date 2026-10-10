//! `DL8_ONE_ENGINE`: work counts with the switch off and on, and two programs compiled on one
//! process engine.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

const LOG: &str = "ivm_sqlite::work=info,ivm_dd=debug,dl8::ir=info,dl8::one=info";
const HELLO: &str = "plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7";
const SECOND: &str = "fixtures/literals/3_str_cons.dl7";
const MARK: &str = "ONE ENGINE SECOND PROGRAM";
const END: &str = "ONE ENGINE SECOND PROGRAM END";
const ENGINES: [&str; 2] = ["ir:dd", "ir:sqlite"];

type Counts = BTreeMap<String, u64>;

fn field(line: &str, name: &str) -> u64 {
    line.split(' ')
        .filter_map(|f| f.split_once('='))
        .find(|(n, _)| *n == name)
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0)
}

fn counts(log: &str) -> Counts {
    let mut out = Counts::new();
    let mut add = |name: &str, value: u64| *out.entry(name.to_string()).or_default() += value;
    for line in log.lines() {
        if line.contains("dl8::ir: ir install") {
            add("installs", 1);
            add("install nodes", field(line, "nodes"));
            if line.contains(" parts=") {
                add("installs, process engine", 1);
            } else if line.contains(" nodes=") {
                add("installs, other", 1);
            }
        } else if line.contains("dl8::ir: ir settle") || line.contains("dl8::ir: ir apply") {
            add("settles", 1);
            add("rows entered", field(line, "sent"));
        } else if line.contains("ivm_dd: dd install") {
            add("dd operators", field(line, "operators"));
            add("dd arrangements", field(line, "arrangements"));
        } else if line.contains("ivm_dd: dd settle") {
            add("dd arranged records", field(line, "arranged_records"));
        } else if line.contains("ivm_sqlite::work: sqlite work") {
            for name in ["creates", "prepared", "vm_steps", "written_d", "written_i", "written_source"] {
                add(&format!("sqlite {name}"), field(line, name));
            }
        } else if line.contains("one engine fallback") {
            add("one engine fallbacks", 1);
        } else if line.contains("one engine separate install") {
            add("one engine separate installs", 1);
        }
    }
    out
}

fn run(args: &[&str], engine: &str, one: &str) -> (Option<i32>, Vec<u8>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .env("DL8_ENGINE", engine)
        .env("DL8_ONE_ENGINE", one)
        .env("RUST_LOG", LOG)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (out.status.code(), out.stdout, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn table(title: &str, columns: &[String], runs: &[Counts]) -> String {
    let mut names: Vec<&String> = runs.iter().flat_map(|c| c.keys()).collect();
    names.sort();
    names.dedup();
    let mut out = format!("### {title}\n| count | {} |\n|---|{}\n", columns.join(" | "), "---|".repeat(columns.len()));
    for name in names {
        let cells: Vec<String> = runs.iter().map(|c| c.get(name).copied().unwrap_or(0).to_string()).collect();
        out += &format!("| {name} | {} |\n", cells.join(" | "));
    }
    out
}

/// Hello world: two installs with the switch off, one with it on, output equal, on both engines.
#[test]
fn hello_world_installs_once() {
    let mut runs = Vec::new();
    let mut columns = Vec::new();
    for engine in ENGINES {
        let off = run(&["compile", HELLO], engine, "0");
        let on = run(&["compile", HELLO], engine, "1");
        assert_eq!(off.0, Some(0), "{engine} off: {}", off.2);
        assert_eq!(on.0, Some(0), "{engine} on: {}", on.2);
        assert!(off.1 == on.1, "{engine}: stdout differs between switch off and on");
        let (off, on) = (counts(&off.2), counts(&on.2));
        assert_eq!(off["installs"], 2, "{engine} off");
        assert_eq!(on["installs"], 1, "{engine} on");
        assert_eq!(on.get("one engine fallbacks"), None, "{engine} on");
        runs.extend([off, on]);
        columns.extend([format!("{engine} off"), format!("{engine} on")]);
    }
    eprintln!("{}", table("hello world", &columns, &runs));
}

/// The registry build, switch off and on, both engines: `registry.rs` identical, counts printed.
#[test]
#[ignore]
fn registry_counts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let oracle = std::fs::read(root.join("hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs")).unwrap();
    let scratch = root.join(format!("target/one-engine-{}", std::process::id()));
    let mut runs = Vec::new();
    let mut columns = Vec::new();
    for engine in ENGINES {
        for one in ["0", "1"] {
            let out = scratch.join(format!("{}-{one}", engine.replace(':', "_")));
            let (code, _, log) = run(&["build", "std/tsi_registry_emit.dl7", "--out", out.to_str().unwrap()], engine, one);
            assert_eq!(code, Some(0), "{engine} {one}: {log}");
            assert!(std::fs::read(out.join("target/tsi/registry.rs")).unwrap() == oracle, "registry {engine} {one}");
            runs.push(counts(&log));
            columns.push(format!("{engine} {}", if one == "1" { "on" } else { "off" }));
        }
    }
    std::fs::remove_dir_all(&scratch).unwrap();
    eprintln!("registry.rs identical in all four runs\n{}", table("registry", &columns, &runs));
}

/// Runs in a child process (`second_program_reuses_the_process_engine`): hello world, then a
/// second program, on one universe and one process engine; the second program's output equals a
/// switch-off compile in a fresh universe.
#[test]
#[ignore]
fn two_programs_child() {
    if std::env::var_os("ONE_ENGINE_CHILD").is_none() {
        return;
    }
    let filter: tracing_subscriber::filter::Targets = LOG.parse().unwrap();
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .with_filter(filter),
    );
    use tracing_subscriber::Layer;
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::env::set_current_dir(root).unwrap();
    let mut u = dl8::_6_eval::Universe::new();
    let mut one = dl8::_8_driver::_7_one::OneEngine::new();
    let first = dl8::compile_in(&mut u, &mut one, Path::new(HELLO), &mut |_| {}).unwrap();
    assert!(first.diagnostics.is_empty());
    eprintln!("{MARK}");
    let second = dl8::compile_in(&mut u, &mut one, Path::new(SECOND), &mut |_| {}).unwrap();
    eprintln!("{END}");
    let shown = |u: &dl8::_6_eval::Universe, c: &dl8::Compile| {
        let mut rows: Vec<String> = c.compiler_rows.iter().map(|r| u.display(*r).to_string()).collect();
        rows.sort();
        (rows, u.display(c.runtime_program).to_string(), c.diagnostics.len())
    };
    let mut alone = dl8::_6_eval::Universe::new();
    let mut fresh = dl8::_8_driver::_7_one::OneEngine::new();
    let expected = dl8::compile_in(&mut alone, &mut fresh, Path::new(SECOND), &mut |_| {}).unwrap();
    assert!(shown(&u, &second) == shown(&alone, &expected), "second program differs from its own compile");
}

/// The second program compiled on the first program's process engine: the process engine is not
/// installed again and no part settles its bootstrap; its own rules may install the compiled fallback.
#[test]
fn second_program_reuses_the_process_engine() {
    let mut runs = Vec::new();
    let mut columns = Vec::new();
    for engine in ENGINES {
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "_9m_one_engine::two_programs_child", "--ignored", "--nocapture", "--test-threads=1"])
            .env("ONE_ENGINE_CHILD", "1")
            .env("DL8_ENGINE", engine)
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "{engine}: {log}");
        let (first, rest) = log.split_once(MARK).expect("marker");
        let (second, _) = rest.split_once(END).expect("end marker");
        let second_log = second;
        let (first, second) = (counts(first), counts(second));
        assert_eq!(first["installs"], 1, "{engine} first program");
        assert_eq!(second.get("installs, process engine"), None, "{engine} second program installs the process engine");
        assert_eq!(second.get("one engine separate installs"), None, "{engine} second program");
        assert_eq!(second.get("one engine fallbacks"), None, "{engine} second program");
        assert!(!second_log.contains("one engine part"), "{engine}: second program bootstrapped a part");
        let alone = run(&["compile", SECOND], engine, "0");
        assert_eq!(alone.0, Some(0), "{engine} second alone");
        runs.extend([first, second, counts(&alone.2)]);
        columns.extend([format!("{engine} first"), format!("{engine} second"), format!("{engine} second alone, off")]);
    }
    eprintln!("{}", table("two programs, one process engine (second: fixtures/literals/3_str_cons.dl7)", &columns, &runs));
}
