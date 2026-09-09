use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");

    let mut git_paths = vec![
        "HEAD".to_string(),
        "index".to_string(),
        "packed-refs".to_string(),
    ];
    if let Some(reference) = command_output("git", &["symbolic-ref", "-q", "HEAD"]) {
        git_paths.push(reference);
    }
    for git_path in git_paths {
        if let Some(path) = command_output("git", &["rev-parse", "--git-path", &git_path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    let git_hash = command_output("git", &["rev-parse", "--short=12", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string());
    let build_datetime =
        command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| {
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            format!("unix:{seconds}")
        });

    println!("cargo:rustc-env=SPREFA_BUILD_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=SPREFA_BUILD_DATETIME={build_datetime}");
}
