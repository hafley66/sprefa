//! Cross-repo call cycles. The built-in `call_edge` resolver scopes callee
//! resolution per-repo (avoids ambiguity), so a call from repo A to a function
//! defined in repo B does NOT appear in `call_edge`. But `call_site` carries
//! the bare callee text and `call_name` carries the global sym→name mapping, so
//! a user-space join resolves cross-repo calls. This test proves the pattern
//! and verifies `scc(edge)` finds the resulting cross-repo cycle.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xrepo_cycle_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Two repos with a mutual call cycle across the boundary:
///   svcA::handle_save -> svcB::persist -> svcA::handle_save
///
/// The built-in `call_edge` is repo-scoped (0 rows for cross-repo calls), so
/// we build a user-space edge rel: join `call_site.callee` (bare text) against
/// `call_name.name` (global, not repo-scoped) to resolve across repos. Then
/// `scc(edge)` finds the 2-member cycle.
#[test]
fn cross_repo_call_cycle_via_user_space_resolution() {
    let d = sandbox("xref");
    let ra = d.join("svc_a");
    let rb = d.join("svc_b");
    fs::create_dir_all(ra.join("src")).unwrap();
    fs::create_dir_all(rb.join("src")).unwrap();
    fs::write(ra.join("src/lib.rs"), "fn handle_save() { persist(); }\n").unwrap();
    fs::write(rb.join("src/lib.rs"), "fn persist() { handle_save(); }\n").unwrap();

    fs::write(
        d.join("cfg.toml"),
        format!(
            "[[repos]]\nslug = \"svc_a\"\nroot = \"{a}\"\n\
         [[repos]]\nslug = \"svc_b\"\nroot = \"{b}\"\n",
            a = ra.display(),
            b = rb.display()
        ),
    )
    .unwrap();

    // User-space cross-repo resolution: call_site.callee (bare text) joined
    // against call_name.name (global) produces edges that span repos.
    let _ = fs::write(
        d.join("p.dl"),
        "\
        rel corpus(p: file).\n\
        corpus(p) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", p, rev).\n\
        rel fns(sym: text).\n\
        fns(sym) <- type_entity(_, sym, _, _, _, _, _).\n\
        rel xref(a: text, b: text).\n\
        xref(caller, callee) <- call_site(_, caller, bare, _, _), call_name(callee, bare).\n\
        rel cyc(rep: text, member: text).\n\
        cyc(rep, member) <- scc(xref).\n\
        rel cyc_size(rep: text, n: int).\n\
        cyc_size(rep, count(member)) <- cyc(rep, member).\n\
        rel cross_cycle(sym: text, rep: text, size: int).\n\
        cross_cycle(member, rep, n) <- cyc(rep, member), cyc_size(rep, n), n > 1.\n\
        ? xref(a, b).\n\
        ? cross_cycle(sym, rep, size).\n",
    );

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(&ra)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "dl failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // xref must show edges in both directions, spanning both repos.
    let xref_block = stdout.split("? xref").nth(1).unwrap_or("");
    assert!(
        xref_block.contains("svc_a") && xref_block.contains("svc_b"),
        "xref edges must carry both repo prefixes:\n{xref_block}\nFULL:\n{stdout}"
    );
    assert!(
        xref_block.contains("handle_save") && xref_block.contains("persist"),
        "xref must resolve both functions:\n{xref_block}"
    );

    // SCC must find the 2-member cycle spanning repos.
    let cyc_block = stdout.split("? cross_cycle").nth(1).unwrap_or("");
    assert!(
        cyc_block.contains("(2 rows)"),
        "exactly 2 members in the cross-repo cycle:\n{cyc_block}"
    );
    assert!(
        cyc_block.contains("svc_a") && cyc_block.contains("svc_b"),
        "cycle members span both repos:\n{cyc_block}"
    );
}

/// Sanity: the built-in call_edge IS repo-scoped — the same cycle produces
/// 0 call_edge rows when the functions are in separate repos. This documents
/// the limitation so a future fix (global resolution in call_edge) can flip
/// the assertion.
#[test]
fn builtin_call_edge_is_repo_scoped_for_cross_repo_calls() {
    let d = sandbox("scoped");
    let ra = d.join("svc_a");
    let rb = d.join("svc_b");
    fs::create_dir_all(ra.join("src")).unwrap();
    fs::create_dir_all(rb.join("src")).unwrap();
    fs::write(ra.join("src/lib.rs"), "fn handle_save() { persist(); }\n").unwrap();
    fs::write(rb.join("src/lib.rs"), "fn persist() { handle_save(); }\n").unwrap();

    fs::write(
        d.join("cfg.toml"),
        format!(
            "[[repos]]\nslug = \"svc_a\"\nroot = \"{a}\"\n\
         [[repos]]\nslug = \"svc_b\"\nroot = \"{b}\"\n",
            a = ra.display(),
            b = rb.display()
        ),
    )
    .unwrap();

    let _ = fs::write(
        d.join("p.dl"),
        "\
        rel corpus(p: file).\n\
        corpus(p) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", p, rev).\n\
        rel fns(sym: text).\n\
        fns(sym) <- type_entity(_, sym, _, _, _, _, _).\n\
        ? call_edge(caller, callee, kind).\n",
    );

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(&ra)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "dl failed: {stdout}");

    let edge_block = stdout.split("? call_edge").nth(1).unwrap_or("");
    assert!(edge_block.contains("(0 rows)"),
        "built-in call_edge must be empty for cross-repo calls (repo-scoped resolver):\n{edge_block}");
}
