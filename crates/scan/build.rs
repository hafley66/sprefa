fn main() {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap_or_else(|_| {
            // Not in a git repo (e.g. vendor/release builds) -- use a sentinel.
            std::process::Output {
                status: std::process::ExitStatus::default(),
                stdout: b"unknown".to_vec(),
                stderr: vec![],
            }
        });

    let hash = String::from_utf8_lossy(&output.stdout);
    let hash = hash.trim();
    let hash = if hash.is_empty() { "unknown" } else { hash };

    println!("cargo:rustc-env=SPREFA_GIT_HASH={hash}");
    // Rebuild when HEAD changes (commit or checkout).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
}
