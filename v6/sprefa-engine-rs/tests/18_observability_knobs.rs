use std::process::Command;

fn startup(binary: &str, arguments: &[&str]) -> serde_json::Value {
    let output = Command::new(binary)
        .args(arguments)
        .env("RUST_LOG", "hafley_observe=debug")
        .env("HAFLEY_LOG_FORMAT", "json")
        .output()
        .expect("run engine binary");
    String::from_utf8(output.stderr)
        .expect("stderr is UTF-8")
        .lines()
        .filter(|line| line.starts_with('{'))
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON event"))
        .find(|value| value["fields"]["message"] == "observability initialized")
        .expect("startup event")
}

#[test]
fn every_engine_binary_uses_the_shared_knobs_and_identity_fields() {
    let actual = [
        startup(env!("CARGO_BIN_EXE_dl6"), &["--help"]),
        startup(env!("CARGO_BIN_EXE_emit_rust_harness"), &[]),
    ]
    .map(|value| {
        serde_json::json!({
            "service.name": value["fields"]["service.name"],
            "service.version": value["fields"]["service.version"],
            "process.pid.is_u64": value["fields"]["process.pid"].is_u64(),
            "log.format": value["fields"]["log.format"],
        })
    });
    assert_eq!(
        actual,
        [
            serde_json::json!({
                "service.name": "sprefa-engine-dl6",
                "service.version": env!("CARGO_PKG_VERSION"),
                "process.pid.is_u64": true,
                "log.format": "json",
            }),
            serde_json::json!({
                "service.name": "sprefa-engine-emit-rust-harness",
                "service.version": env!("CARGO_PKG_VERSION"),
                "process.pid.is_u64": true,
                "log.format": "json",
            }),
        ]
    );
}
