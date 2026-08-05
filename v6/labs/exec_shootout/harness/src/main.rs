use exec_shootout_harness::gen::{self, Family};
use exec_shootout_harness::refengine::read_input;
use exec_shootout_harness::runner::{self, EngineEvent, RunOutcome};
use exec_shootout_harness::standings::{self, BuildRow, CaseStanding, EngineRow, RunMeta};
use exec_shootout_harness::tuner;
use std::io::Write;
use std::path::Path;

struct Engine {
    name: String,
    path: String,
    is_reference: bool,
    size_bytes: u64,
}

fn exe_dir() -> String {
    std::env::current_exe()
        .expect("current exe")
        .parent()
        .expect("exe parent dir")
        .to_string_lossy()
        .to_string()
}

fn resolve_engines(tokens: &[String]) -> Vec<Engine> {
    let dir = exe_dir();
    tokens
        .iter()
        .map(|token| {
            if token == "ref" {
                let path = format!("{}/ref_engine", dir);
                Engine {
                    name: "ref".to_string(),
                    path: path.clone(),
                    is_reference: true,
                    size_bytes: file_size(&path),
                }
            } else {
                let path = decode_path(token);
                let name = Path::new(&path)
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_else(|| token.clone());
                Engine {
                    name,
                    path: path.clone(),
                    is_reference: false,
                    size_bytes: file_size(&path),
                }
            }
        })
        .collect()
}

fn decode_path(token: &str) -> String {
    if token.starts_with('/') || token.contains('/') {
        token.to_string()
    } else {
        token.to_string()
    }
}

fn file_size(path: &str) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn write_input(path: &str, nodes: u32, edges: &[(u32, u32)]) {
    let mut file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("cannot create {}: {}", path, error));
    writeln!(file, "p {} {}", nodes, edges.len()).expect("write header");
    for (from, to) in edges {
        writeln!(file, "{} {}", from, to).expect("write edge");
    }
}

fn best_of_three(
    binary: &str,
    input: &str,
    runs: u32,
    timeout: std::time::Duration,
) -> (Option<RunOutcome>, bool) {
    let mut best: Option<RunOutcome> = None;
    let mut any_failure = false;
    for _ in 0..runs {
        match runner::run_engine(binary, input, timeout) {
            Ok(outcome) => {
                let replace = match &best {
                    Some(prior) => outcome.event.fixpoint_ms < prior.event.fixpoint_ms,
                    None => true,
                };
                if replace {
                    best = Some(outcome);
                }
            }
            Err(error) => {
                eprintln!("engine run failed for {}: {}", binary, error);
                any_failure = true;
            }
        }
    }
    (best, any_failure)
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mut engine_tokens: Option<Vec<String>> = None;
    let mut scales: Vec<u32> = vec![10_000, 100_000, 1_000_000];
    let mut work_dir = format!("{}/work", exe_dir());
    let mut standings_path = default_standings_path();
    let mut measure_builds = false;

    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.as_str() {
            "--engines" => {
                index += 1;
                engine_tokens = Some(
                    arguments[index]
                        .split(',')
                        .map(|token| token.trim().to_string())
                        .filter(|token| !token.is_empty())
                        .collect(),
                );
            }
            "--scales" => {
                index += 1;
                scales = arguments[index]
                    .split(',')
                    .map(|token| token.trim().parse().expect("scale not an int"))
                    .collect();
            }
            "--work" => {
                index += 1;
                work_dir = arguments[index].clone();
            }
            "--standings" => {
                index += 1;
                standings_path = arguments[index].clone();
            }
            "--measure-builds" => {
                measure_builds = true;
            }
            "--help" | "-h" => {
                eprintln!("harness --engines <bin[,[bin...]]|[ref]> [--scales a,b,c] [--work DIR] [--standings PATH] [--measure-builds]");
                std::process::exit(0);
            }
            _ => {
                eprintln!("harness: unknown argument '{}'", argument);
                std::process::exit(1);
            }
        }
        index += 1;
    }

    let engine_tokens = engine_tokens.unwrap_or_else(|| {
        eprintln!("harness: missing --engines");
        std::process::exit(1);
    });
    let engines = resolve_engines(&engine_tokens);
    std::fs::create_dir_all(&work_dir).expect("create work dir");
    if !Path::new(&standings_path)
        .parent()
        .map_or(true, std::path::Path::exists)
    {
        eprintln!(
            "harness: standings parent dir does not exist: {}",
            standings_path
        );
        std::process::exit(1);
    }

    let command_line = std::env::args().collect::<Vec<_>>().join(" ");
    let meta = RunMeta { command_line };
    let timeout = runner::DEFAULT_TIMEOUT;

    let mut build_rows: Vec<BuildRow> = Vec::new();

    if measure_builds {
        for engine in &engines {
            if engine.is_reference {
                continue;
            }
            if let Some(seconds) = measure_build(&engine.path) {
                build_rows.push(BuildRow {
                    name: engine.name.clone(),
                    size_bytes: engine.size_bytes,
                    build_seconds: Some(seconds),
                });
            }
        }
    }

    let families = [Family::Chain, Family::Layered, Family::Grid];
    let mut cases: Vec<CaseStanding> = Vec::new();
    let mut had_failure = false;

    for family in families {
        for scale in &scales {
            let scale = *scale;
            let tuned = tuner::tune(family, scale);
            let generated = gen::generate(family, tuned.params, scale, scale as u64 ^ 0x5eed_cafe);
            let input_path = format!("{}/{}_{}.in", work_dir, gen::family_name(family), scale);
            write_input(&input_path, generated.node_count, &generated.edges);

            let file_nodes = read_input(&input_path).0;
            if file_nodes != generated.node_count {
                eprintln!("harness: input node count mismatch");
                std::process::exit(1);
            }

            let mut engine_rows: Vec<EngineRow> = Vec::new();
            let mut participants: Vec<(String, EngineEvent)> = Vec::new();

            for engine in &engines {
                let runs = if engine.is_reference { 1 } else { 3 };
                let (best, any_failure) = best_of_three(&engine.path, &input_path, runs, timeout);
                if any_failure {
                    had_failure = true;
                }
                match best {
                    Some(outcome) => {
                        engine_rows.push(EngineRow {
                            name: engine.name.clone(),
                            is_reference: engine.is_reference,
                            derived: outcome.event.derived,
                            fixpoint_ms: outcome.event.fixpoint_ms,
                            load_ms: outcome.event.load_ms,
                            peak_rss_kb: outcome.event.peak_rss_kb,
                            runs_used: runs,
                            dnf: false,
                        });
                        participants.push((engine.name.clone(), outcome.event));
                    }
                    None => {
                        engine_rows.push(EngineRow {
                            name: engine.name.clone(),
                            is_reference: engine.is_reference,
                            derived: 0,
                            fixpoint_ms: 0,
                            load_ms: 0,
                            peak_rss_kb: 0,
                            runs_used: 0,
                            dnf: true,
                        });
                    }
                }
            }

            let mut truth: Option<EngineEvent> = None;
            if scale == 10_000 {
                let ref_path = format!("{}/ref_engine", exe_dir());
                let outcome =
                    runner::run_engine(&ref_path, &input_path, timeout).unwrap_or_else(|error| {
                        eprintln!(
                            "harness: reference engine failed on {}: {}",
                            input_path, error
                        );
                        std::process::exit(1);
                    });
                truth = Some(outcome.event);
            }

            if let Some(reference_event) = &truth {
                for (engine_name, event) in &participants {
                    if event.derived != reference_event.derived
                        || event.checksum != reference_event.checksum
                    {
                        eprintln!(
                            "harness: MISMATCH at {} scale={} engine={}: engine (derived={} checksum={}) vs reference (derived={} checksum={})",
                            gen::family_name(family),
                            scale,
                            engine_name,
                            event.derived,
                            event.checksum,
                            reference_event.derived,
                            reference_event.checksum
                        );
                        std::process::exit(1);
                    }
                }
                let reference_participant = engines.iter().any(|engine| engine.is_reference);
                if !reference_participant {
                    engine_rows.push(EngineRow {
                        name: "ref".to_string(),
                        is_reference: true,
                        derived: reference_event.derived,
                        fixpoint_ms: 0,
                        load_ms: 0,
                        peak_rss_kb: 0,
                        runs_used: 1,
                        dnf: false,
                    });
                }
            } else {
                if participants.len() >= 2 {
                    let baseline = &participants[0].1;
                    for (engine_name, event) in participants.iter().skip(1) {
                        if event.derived != baseline.derived || event.checksum != baseline.checksum
                        {
                            eprintln!(
                                "harness: MISMATCH at {} scale={} engine={} vs {}: derived {} vs {}",
                                gen::family_name(family),
                                scale,
                                engine_name,
                                participants[0].0,
                                event.derived,
                                baseline.derived
                            );
                            std::process::exit(1);
                        }
                    }
                }
            }

            cases.push(CaseStanding {
                family_name: gen::family_name(family).to_string(),
                scale,
                edges: tuned.edge_count,
                derived: tuned.derived,
                params_label: gen::params_label(family, tuned.params),
                rows: engine_rows,
            });
        }
    }

    for engine in &engines {
        if engine.is_reference {
            continue;
        }
        if build_rows.iter().any(|build| build.name == engine.name) {
            continue;
        }
        build_rows.push(BuildRow {
            name: engine.name.clone(),
            size_bytes: engine.size_bytes,
            build_seconds: None,
        });
    }

    standings::write_standings_to(&standings_path, &cases, &build_rows, &meta);
    eprintln!(
        "harness: run complete; standings written to {}",
        standings_path
    );
    if had_failure {
        eprintln!("harness: one or more engine runs failed; exiting nonzero");
        std::process::exit(1);
    }
}

fn default_standings_path() -> String {
    let dir = exe_dir();
    let mut current = Path::new(&dir);
    loop {
        if current.join("CONTRACT.md").exists() {
            return current.join("STANDINGS.md").to_string_lossy().to_string();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    format!("{}/STANDINGS.md", dir)
}

fn measure_build(binary: &str) -> Option<f64> {
    let binary_path = Path::new(binary);
    let binary_dir = binary_path.parent()?;
    let mut crate_dir = binary_dir.to_path_buf();
    while !crate_dir.join("Cargo.toml").exists() {
        if !crate_dir.pop() {
            return None;
        }
    }
    let start = std::time::Instant::now();
    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&crate_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    Some(start.elapsed().as_secs_f64())
}
