//! `const { factory } = context;` then `factory.createX()`. `receiver_of`
//! read `BindingIdentifier` declarators only, so an object pattern bound
//! nothing and the site fell to a name match. Each property now binds to its
//! base's declared type one hop out, the same `RecvSpec::Field` shape the
//! `base.field.recv()` leg already used, and the property's type is found on
//! the base or on a base it extends.
//!
//! A destructured receiver is a GUESS, so a site it fails to bind falls back
//! to the name match; a directly declared receiver still owns its site. That
//! policy is what the corpus measured: owning the site cost 101 oracle rows.
//!
//! Fail-first at HEAD (ef6b47d91), `--resolve` over the three fixtures: no
//! edges at all, the decoy having made `writeLine` ambiguous by name.
//!
//! Expected values are hand-derived from the fixtures.

use std::process::Command;

const DIR: &str = "tests/fixtures/ts5_findings/destructured_receiver";

fn resolved_edges() -> Vec<(String, String, String)> {
    let root = env!("CARGO_MANIFEST_DIR");
    let files = ["ctx.ts", "use.ts", "decoy.ts"];
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(files.map(|file| format!("{root}/{DIR}/{file}")))
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf8 wire")
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "resolved_edge").then(|| {
                (
                    row["caller_name"].as_str().unwrap_or("").to_string(),
                    row["callee_name"].as_str().unwrap_or("").to_string(),
                    row["callee_path"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

fn binds(edges: &[(String, String, String)], caller: &str) -> usize {
    edges
        .iter()
        .filter(|(c, callee, path)| {
            c == caller && callee == "writeLine" && path.ends_with("ctx.ts")
        })
        .count()
}

/// The destructured binding and the member read of the same property reach one
/// interface signature: two sites in `transform`, one target.
#[test]
fn a_destructured_property_binds_like_a_member_read() {
    let edges = resolved_edges();
    assert_eq!(binds(&edges, "transform"), 2, "{edges:?}");
}

/// `const { emitter: sink } = context`: the LOCAL name is what the scope binds,
/// the PROPERTY name is what the field hop reads.
#[test]
fn a_renamed_destructured_property_binds_under_its_local_name() {
    let edges = resolved_edges();
    assert_eq!(binds(&edges, "transformRenamed"), 1, "{edges:?}");
}

/// The decoy declares `writeLine` too, so a name match cannot answer: nothing
/// in this fixture binds into decoy.ts.
#[test]
fn the_decoy_never_wins_a_member_site() {
    let edges = resolved_edges();
    assert!(
        !edges.iter().any(|(_, _, path)| path.ends_with("decoy.ts")),
        "a name match reached the decoy: {edges:?}"
    );
}
