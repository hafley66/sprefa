//! `DL8_MACROTIME=rust` and `engine` agree on the cases that stop the waves (a repeated graph,
//! the 64th rewriting wave, the 63rd), and their work counts side by side.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

const ENGINES: [&str; 2] = ["ir:sqlite", "ir:dd"];
const MODES: [&str; 2] = ["rust", "engine"];

fn expand(stem: &str, engine: &str, mode: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(root)
        .arg("expand")
        .arg(root.join(format!("fixtures/macrotime_waves/{stem}.json")))
        .env("DL8_ENGINE", engine)
        .env("DL8_MACROTIME", mode)
        .output()
        .unwrap()
        .stdout
}

/// The output, identical across engines and modes.
fn agreed_bytes(stem: &str) -> Vec<u8> {
    let mut outputs = Vec::new();
    for engine in ENGINES {
        for mode in MODES {
            outputs.push(((engine, mode), expand(stem, engine, mode)));
        }
    }
    let first = &outputs[0].1;
    for (at, bytes) in &outputs {
        assert!(
            bytes == first,
            "{stem} {at:?} differs from {:?}",
            outputs[0].0
        );
    }
    outputs.swap_remove(0).1
}

/// The diagnostics and expanded row count, identical across engines and modes.
fn agreed(stem: &str) -> (Value, usize) {
    let bytes = agreed_bytes(stem);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let rows = value["expanded_rows"].as_array().unwrap().len();
    (value["diagnostics"].clone(), rows)
}

#[test]
fn rewrite_to_the_same_graph_is_a_cycle() {
    let (diagnostics, rows) = agreed("0_cycle");
    assert_eq!(
        (diagnostics.to_string(), rows),
        (
            r#"[{"args":[{"a":"macrotime"},{"a":"none"},{"args":[0],"f":"expansion_cycle"}],"f":"diagnostic"}]"#.to_string(),
            0
        )
    );
}

#[test]
fn a_64th_rewriting_wave_is_the_round_limit() {
    let (diagnostics, rows) = agreed("1_round_limit");
    assert_eq!(
        (diagnostics.to_string(), rows),
        (
            r#"[{"args":[{"a":"macrotime"},{"a":"none"},{"args":[64],"f":"expansion_round_limit"}],"f":"diagnostic"}]"#.to_string(),
            0
        )
    );
}

#[test]
fn a_63rd_rewriting_wave_settles() {
    // 63 nested brackets nest the JSON past serde_json's depth limit: read the text.
    let bytes = agreed_bytes("2_last_wave");
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(&text[..39], r#"{"diagnostics":[],"expanded_rows":[{"ar"#);
    assert_eq!(text.matches(r#""f":"node"}"#).count(), 387);
}

const RUNS: [(&str, &str); 4] = [
    ("ir:sqlite", "rust"),
    ("ir:sqlite", "engine"),
    ("ir:dd", "rust"),
    ("ir:dd", "engine"),
];
const WORK_LOG: &str = "ivm_sqlite::work=info,ivm_dd=debug,dl8::macrotime=info";

type Counts = std::collections::BTreeMap<String, u64>;

fn run_logged(
    args: &[&std::ffi::OsStr],
    engine: &str,
    mode: &str,
) -> (Option<i32>, Vec<u8>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .env("DL8_ENGINE", engine)
        .env("DL8_MACROTIME", mode)
        .env("RUST_LOG", WORK_LOG)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (
        out.status.code(),
        out.stdout,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn is_install(line: &str) -> bool {
    line.contains("phase=\"install\"") || line.contains("ivm_dd: dd install")
}

/// The lines from the install before the first `macrotime prepare` up to the next install after
/// the last `macrotime unit` line: the macrotime session's events.
fn macrotime_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let Some(first) = lines.iter().position(|l| l.contains("macrotime prepare")) else {
        return Vec::new();
    };
    let start = (0..first)
        .rev()
        .find(|i| is_install(lines[*i]))
        .unwrap_or(0);
    let last = lines
        .iter()
        .rposition(|l| l.contains("macrotime unit"))
        .unwrap();
    let end = (last + 1..lines.len())
        .find(|i| is_install(lines[*i]))
        .unwrap_or(lines.len());
    lines[start..end].to_vec()
}

/// `ivm_sqlite::work` fields per phase and in total; `dd install` operators, nodes and
/// arrangements; `dd settle` changes, epochs, steps and arranged records.
fn add_counts(counts: &mut Counts, lines: &[&str]) {
    for line in lines {
        let fields: Vec<(&str, &str)> = line.split(' ').filter_map(|f| f.split_once('=')).collect();
        let field = |name: &str| {
            fields
                .iter()
                .find(|(n, _)| *n == name)
                .and_then(|(_, v)| v.parse::<u64>().ok())
                .unwrap_or(0)
        };
        if line.contains("ivm_sqlite::work: sqlite work") {
            let phase = fields
                .iter()
                .find(|(n, _)| *n == "phase")
                .unwrap()
                .1
                .trim_matches('"');
            for (name, value) in &fields {
                if let Ok(value) = value.parse::<u64>() {
                    *counts.entry(format!("sqlite {phase} {name}")).or_default() += value;
                    *counts.entry(format!("sqlite total {name}")).or_default() += value;
                }
            }
        } else if line.contains("ivm_dd: dd install") {
            for name in ["operators", "nodes", "arrangements"] {
                *counts.entry(format!("dd install {name}")).or_default() += field(name);
            }
        } else if line.contains("ivm_dd: dd settle") {
            for name in [
                "changes_in",
                "changes_out",
                "epochs",
                "steps",
                "arranged_records",
            ] {
                *counts.entry(format!("dd settle {name}")).or_default() += field(name);
            }
        }
    }
}

/// One markdown table, a column per run; of the sqlite counts only the totals.
fn table(title: &str, columns: &[Counts]) -> String {
    let mut names: std::collections::BTreeSet<&String> =
        columns.iter().flat_map(|c| c.keys()).collect();
    names.retain(|name| !name.starts_with("sqlite ") || name.starts_with("sqlite total "));
    let mut out = format!("\n### {title}\n\n| count |");
    for (engine, mode) in RUNS {
        out += &format!(" {engine} {mode} |");
    }
    out += "\n|---|---|---|---|---|\n";
    for name in names {
        out += &format!("| {name} |");
        for counts in columns {
            match counts.get(name) {
                Some(value) => out += &format!(" {value} |"),
                None => out += " - |",
            }
        }
        out += "\n";
    }
    out
}

/// Registry build and census (`dl8 compile` of every `.dl7` under fixtures examples
/// book/src/probes std) in each mode: outputs equal, counts printed.
#[test]
#[ignore]
fn work_counts_rust_vs_engine() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let oracle =
        std::fs::read(root.join("hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs")).unwrap();
    let scratch = root.join(format!(
        "target/macrotime-waves-work-{}",
        std::process::id()
    ));
    let mut total = Vec::new();
    let mut macrotime = Vec::new();
    for (engine, mode) in RUNS {
        let out = scratch.join(format!("{}-{mode}", engine.replace(':', "_")));
        let (code, _, log) = run_logged(
            &[
                "build".as_ref(),
                "std/tsi_registry_emit.dl7".as_ref(),
                "--out".as_ref(),
                out.as_os_str(),
            ],
            engine,
            mode,
        );
        assert_eq!(code, Some(0), "registry {engine} {mode}");
        assert!(
            std::fs::read(out.join("target/tsi/registry.rs")).unwrap() == oracle,
            "registry {engine} {mode}"
        );
        let lines: Vec<&str> = log.lines().collect();
        let (mut all, mut macro_counts) = (Counts::new(), Counts::new());
        add_counts(&mut all, &lines);
        add_counts(&mut macro_counts, &macrotime_lines(&lines));
        total.push(all);
        macrotime.push(macro_counts);
    }
    std::fs::remove_dir_all(&scratch).unwrap();
    let mut report = String::from("registry: exit 0 and registry.rs identical in all four runs\n");
    report += &table("registry, total", &total);
    report += &table("registry, macrotime session", &macrotime);

    let mut programs = Vec::new();
    for dir in ["fixtures", "examples", "book/src/probes", "std"] {
        let mut todo = vec![root.join(dir)];
        while let Some(at) = todo.pop() {
            for entry in std::fs::read_dir(&at).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    todo.push(path);
                } else if path.extension().is_some_and(|x| x == "dl7") {
                    programs.push(path.strip_prefix(root).unwrap().to_path_buf());
                }
            }
        }
    }
    programs.sort();
    let mut total = vec![Counts::new(); RUNS.len()];
    let mut macrotime = vec![Counts::new(); RUNS.len()];
    let (mut clean, mut differ) = (0, Vec::new());
    for program in &programs {
        let runs: Vec<_> = RUNS
            .iter()
            .map(|(engine, mode)| {
                run_logged(&["compile".as_ref(), program.as_os_str()], engine, mode)
            })
            .collect();
        for pair in [(0, 1), (2, 3)] {
            if runs[pair.0].0 != runs[pair.1].0 || runs[pair.0].1 != runs[pair.1].1 {
                differ.push(format!("{} {}", program.display(), RUNS[pair.0].0));
            }
        }
        if runs.iter().all(|run| run.0 == Some(0)) {
            clean += 1;
            for (at, run) in runs.iter().enumerate() {
                let lines: Vec<&str> = run.2.lines().collect();
                add_counts(&mut total[at], &lines);
                add_counts(&mut macrotime[at], &macrotime_lines(&lines));
            }
        }
    }
    report += &format!(
        "\ncensus: {} programs, exit 0 in all four runs: {clean}, stdout or exit differ between modes: {differ:?}\n",
        programs.len()
    );
    report += &table(&format!("census ({clean} programs), total"), &total);
    report += &table(
        &format!("census ({clean} programs), macrotime session"),
        &macrotime,
    );
    println!("{report}");
    assert!(differ.is_empty(), "{differ:?}");
}
