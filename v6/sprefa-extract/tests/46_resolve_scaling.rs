//! `--resolve` scaling on a module-sized file set: wall time must grow
//! linearly with file count (the x/text module dirs timed out pre-F2; with
//! buffered emission the whole 486-file module resolves in 3.34s). This test
//! pins the growth ratio n=400 vs n=200 under 2.5.

use std::process::Command;
use std::time::Instant;

const RATIO_BUDGET: f64 = 2.5;

fn module_files(dir: &std::path::Path, n: usize) -> Vec<String> {
    let mut args = Vec::new();
    for i in 0..n {
        let path = dir.join(format!("f{i}.go"));
        std::fs::write(
            &path,
            format!(
                "package m\n\nvar v{i} int = {i}\n\nfunc get{i}() int {{ return v{i} }}\n\nfunc use{i}() int {{ return get{i}() }}\n"
            ),
        )
        .unwrap();
        args.push(path.to_string_lossy().into_owned());
    }
    args
}

fn resolve_wall(bin: &str, args: &[String]) -> f64 {
    let t = Instant::now();
    let out = Command::new(bin)
        .arg("--resolve")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "resolve failed: {:?}", out.stderr);
    t.elapsed().as_secs_f64()
}

#[test]
fn resolve_wall_grows_linearly_with_file_count() {
    let dir = std::env::temp_dir().join("sprefa-extract-46-resolve");
    std::fs::create_dir_all(&dir).unwrap();
    let bin = env!("CARGO_BIN_EXE_extract");

    let args200 = module_files(&dir, 200);
    let args400 = module_files(&dir, 400);

    let wall200 = resolve_wall(bin, &args200);
    let wall400 = resolve_wall(bin, &args400);

    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}
