// The `data` family on the Rust door. The in-process mask parser is the only
// place a family name is spelled twice (the other is extract's own parse_mask),
// so this pins the two spellings to one behaviour on the corpus spec.
//
// dl6: v6/dl/fixtures/openapi-data-family.dl6, whose header records the TS
// door's counts on the same spec: spec_doc 1, spec_operation 100, spec_path 100.
// rx: specFile$.pipe(map(mintIdentityAndWitness), distinct(d => d.witnessDigest),
//     mergeMap(runExtract), mergeMap(commitEdbArrival)), then specDoc$ fanned
//     out by decode's two nested key holes.

use std::process::Command;

use sprefa_engine_rs::driver::run_schedule_live;
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{Arrival, ArrivalSign, ProgramJson, SqlStatement, Value};
use sprefa_engine_rs::GenProgram;

fn compile_fixture(name: &str) -> GenProgram {
    let engine = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = engine.join(format!("../dl/fixtures/{name}.dl6"));
    let compile = engine.join("../prolog/compile.pl");
    let emit_rust = engine.join("../prolog/emit_rust.pl");
    let temp = tempfile::tempdir().expect("temporary compiler output directory");
    let generated = temp.path().join(format!("{name}.program.rs"));
    let goal = format!(
        "compile_dl6('{}','{}',[emitter(emit_rust:emit_program)])",
        source.display(),
        generated.display()
    );
    let output = Command::new("swipl")
        .args(["-q", "-l"])
        .arg(&compile)
        .args(["-l"])
        .arg(&emit_rust)
        .args(["-g", &goal, "-g", "halt"])
        .output()
        .expect("run compile_dl6 for the Rust emitter");
    assert!(
        output.status.success(),
        "compile_dl6 failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let module_text = std::fs::read_to_string(&generated).expect("read emitted module");
    let start = module_text.find("r#\"").expect("raw string open") + 3;
    let end = module_text[start..].find("\"#;").expect("raw string close") + start;
    let program_json: ProgramJson =
        serde_json::from_str(&module_text[start..end]).expect("emitted program json");
    GenProgram::from_json(program_json)
}

fn row_count(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> usize {
    let select = program.final_select.get(rel).expect("final select for rel");
    seam.execute(&SqlStatement {
        sql: format!("SELECT * FROM ({select})"),
        args: vec![],
    })
    .expect("read rel")
    .rows
    .len()
}

// FAIL-PRE-FIX: the run stopped rc=101 with `sh host 'extract': family `data`
// is not a known family; in-process families are cst, type, call, df`.
#[tokio::test]
async fn the_rust_door_reads_an_openapi_spec_through_the_data_family() {
    let program = compile_fixture("openapi-data-family");
    let seam = SqliteSeam::in_memory().expect("seam");
    let spec = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../dl/fixtures/pokeapi.openapi.yml")
        .canonicalize()
        .expect("corpus spec path");
    // An empty digest names the no-digest branch: the host reads the worktree
    // bytes, so no repository lookup enters this run.
    let schedule = vec![vec![Arrival {
        rel: "spec_file".to_string(),
        sign: ArrivalSign::Add,
        row: vec![
            Value::Text(spec.display().to_string()),
            Value::Text(String::new()),
        ],
    }]];
    run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run of the data family");

    assert_eq!(row_count(&program, &seam, "spec_doc"), 1);
    assert_eq!(
        row_count(&program, &seam, "spec_operation"),
        100,
        "the Rust door must fan out the same operations the TS door counts"
    );
    assert_eq!(row_count(&program, &seam, "spec_path"), 100);
}
