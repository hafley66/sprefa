//! The go interface dispatch fan-out: a call site `x.M()` whose receiver is
//! an interface keeps the `I.M` spec edge and gains one `implements` edge per
//! implementer of `I` in the corpus, same call site. An interface with more
//! than 64 implementers emits the spec edge only plus one `unresolved` row
//! reason `fanout_cap`.

use std::collections::BTreeSet;
use std::process::Command;

const DIR: &str = "tests/fixtures/go_findings/iface_fanout";

/// (caller_name, callee_name, kind, caller_site_start) per resolved edge.
fn resolved_edges(fixture: &str) -> Vec<(String, String, String, u64)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg(env!("CARGO_MANIFEST_DIR").to_owned() + "/" + DIR + "/" + fixture)
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
                    row["kind"].as_str().unwrap_or("").to_string(),
                    row["caller_site_start"].as_u64().unwrap_or(0),
                )
            })
        })
        .collect()
}

fn unresolved(fixture: &str) -> Vec<(String, String)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg(env!("CARGO_MANIFEST_DIR").to_owned() + "/" + DIR + "/" + fixture)
        .output()
        .expect("extract binary runs");
    String::from_utf8(out.stdout)
        .expect("utf8 wire")
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "unresolved").then(|| {
                (
                    row["reason"].as_str().unwrap_or("").to_string(),
                    row["detail"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

/// Two implementers -> each site keeps the spec edge and gains one fan-out
/// edge per implementer (kind `implements`), same call site.
#[test]
fn two_implementers_fan_out_per_site() {
    let edges = resolved_edges("two_impls.go");
    let m_sites: BTreeSet<u64> = edges
        .iter()
        .filter(|(caller, callee, _, _)| caller == "draw" && callee == "M")
        .map(|(_, _, _, site)| *site)
        .collect();
    assert_eq!(m_sites.len(), 1, "one M site: {edges:?}");
    let site = *m_sites.iter().next().unwrap();
    let at_site: BTreeSet<(String, String)> = edges
        .iter()
        .filter(|(_, _, _, s)| *s == site)
        .map(|(caller, callee, kind, _)| (callee.clone(), kind.clone()))
        .collect();
    assert_eq!(
        at_site,
        BTreeSet::from([
            ("M".to_string(), "name_resolve".to_string()),
            ("M".to_string(), "implements".to_string()),
        ]),
        "spec edge stays and implementers fan out: {edges:?}"
    );
    let n_sites: BTreeSet<u64> = edges
        .iter()
        .filter(|(caller, callee, _, _)| caller == "draw" && callee == "N")
        .map(|(_, _, _, site)| *site)
        .collect();
    assert_eq!(n_sites.len(), 1, "one N site: {edges:?}");
    let n_site = *n_sites.iter().next().unwrap();
    let fanout_targets: BTreeSet<String> = edges
        .iter()
        .filter(|(caller, _, kind, s)| {
            caller == "draw" && kind == "implements" && (*s == site || *s == n_site)
        })
        .map(|(_, callee, _, s)| format!("{s}->{callee}"))
        .collect();
    assert_eq!(
        fanout_targets,
        BTreeSet::from([
            format!("{site}->M"),
            format!("{n_site}->N"),
        ]),
        "one fan-out edge per implementer per site: {edges:?}"
    );
}

/// An implementer missing one method is excluded: `Half` covers only `Open`,
/// so the `Close` site fans out to `Full` alone.
#[test]
fn implementer_missing_a_method_is_excluded() {
    let edges = resolved_edges("missing_method.go");
    let sites: BTreeSet<u64> = edges
        .iter()
        .filter(|(caller, callee, kind, _)| {
            caller == "swing" && callee == "Close" && kind == "name_resolve"
        })
        .map(|(_, _, _, site)| *site)
        .collect();
    assert_eq!(sites.len(), 1, "one Close site: {edges:?}");
    let site = *sites.iter().next().unwrap();
    let fanout: BTreeSet<String> = edges
        .iter()
        .filter(|(caller, _, kind, s)| caller == "swing" && kind == "implements" && *s == site)
        .map(|(_, callee, _, _)| callee.clone())
        .collect();
    assert_eq!(
        fanout,
        BTreeSet::from(["Close".to_string()]),
        "Half implements nothing fully; only Full fans out: {edges:?}"
    );
}

/// 65 implementers exceed the cap: the spec edge stays, no fan-out edge is
/// emitted, and one `unresolved` row reason `fanout_cap` carries the count.
#[test]
fn over_the_cap_emits_fanout_cap_and_no_fanout() {
    let edges = resolved_edges("cap65.go");
    let at_site: Vec<_> = edges
        .iter()
        .filter(|(caller, callee, kind, _)| {
            caller == "pingAll" && callee == "Ping" && kind != "import_resolve"
        })
        .collect();
    assert_eq!(at_site.len(), 1, "spec edge only: {edges:?}");
    assert_eq!(at_site[0].2, "name_resolve");
    let caps: Vec<_> = unresolved("cap65.go")
        .into_iter()
        .filter(|(reason, _)| reason == "fanout_cap")
        .collect();
    assert_eq!(caps.len(), 1, "one cap row: {caps:?}");
    assert!(caps[0].1.contains("65"), "detail carries the count: {caps:?}");
}
