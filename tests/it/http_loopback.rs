//! HTTP loopback virtual edges: a function calls the service's own API via
//! `fetch("/api/...")`, which routes to a handler that calls the original
//! function — a cycle invisible to `call_edge` (the HTTP call is a runtime
//! dispatch, not a source-level function call).
//!
//! The test builds the virtual edge in user-space dl:
//!   1. `match()` finds `app.get("/api/users", handleUsers)` → route(url, handler)
//!   2. `match()` + `call_site` finds `fetch("/api/users")` → fetch_edge(caller, url)
//!   3. join fetch URL = route URL → http_edge(caller, handler)
//!   4. union http_edge with call_edge, run scc() → the cycle appears

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("http_loopback_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &std::path::Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The virtual cycle:
///   handleUsers --calls--> saveConfig --fetch("/api/users")--> handleUsers
/// The real call_edge sees handleUsers→saveConfig. The http_edge (virtual)
/// adds saveConfig→handleUsers. The union has a cycle the scc finds.
#[test]
fn http_loopback_creates_virtual_cycle() {
    let d = sandbox("cycle");
    // TS fixture: two functions + a route registration that closes the loop.
    fs::write(d.join("src/app.ts"),
        "function handleUsers() {\n\
         \x20\x20\x20\x20saveConfig();\n\
         }\n\
         function saveConfig() {\n\
         \x20\x20\x20\x20fetch(\"/api/users\");\n\
         }\n\
         app.get(\"/api/users\", handleUsers);\n").unwrap();

    let prog = r#"
rel fns(sym: text).
fns(sym) <- type_entity(_, sym, _, _, _, _, _).

# 1. Route registration: app.get("/api/users", handleUsers)
rel route(url: text, handler: text).
route(url, handler) <-
    scan("WORK", "src/**/*.ts", p, rev),
    match(p, rev, /app\.get\("(?<url>[^"]+)", (?<handler>\w+)\)/, l).

# 2. Fetch call site: find lines with fetch("..."), join call_site for caller.
rel fetch_line(file: file, line: int, url: text).
fetch_line(p, l, url) <-
    scan("WORK", "src/**/*.ts", p, rev),
    match(p, rev, /fetch\("(?<url>[^"]+)"\)/, l).

rel fetch_edge(caller: text, url: text).
fetch_edge(caller, url) <-
    call_site(_, caller, "fetch", p, l),
    fetch_line(p, l, url).

# 3. Virtual HTTP edge: fetch URL matches a route URL → caller→handler.
rel http_edge(caller: text, handler: text).
http_edge(caller, handler_sym) <-
    fetch_edge(caller, url),
    route(url, handler_name),
    type_entity(_, handler_sym, handler_name, _, _, _, _).

# 4. Union real call_edge + virtual http_edge.
rel all_edge(a: text, b: text).
all_edge(caller, callee) <- call_edge(caller, callee, _).
all_edge(caller, handler) <- http_edge(caller, handler).

# 5. SCC over the union.
rel cyc(rep: text, member: text).
cyc(rep, member) <- scc(all_edge).
rel cyc_size(rep: text, n: int).
cyc_size(rep, count(member)) <- cyc(rep, member).
rel loop_cycle(sym: text, size: int).
loop_cycle(member, n) <- cyc(rep, member), cyc_size(rep, n), n > 1.

? route(url, handler).
? fetch_edge(caller, url).
? http_edge(caller, handler).
? all_edge(a, b).
? loop_cycle(sym, size).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "dl failed:\n{err}");

    // Route extracted.
    let rt = out.split("? route").nth(1).unwrap_or("");
    assert!(rt.contains("/api/users") && rt.contains("handleUsers"),
        "route must be extracted:\n{rt}");

    // Fetch call attributed to saveConfig.
    let fe = out.split("? fetch_edge").nth(1).unwrap_or("");
    assert!(fe.contains("/api/users") && fe.contains("saveConfig"),
        "fetch_edge must link saveConfig to the URL:\n{fe}");

    // Virtual http_edge resolves to handleUsers.
    let he = out.split("? http_edge").nth(1).unwrap_or("");
    assert!(he.contains("saveConfig") && he.contains("handleUsers"),
        "http_edge must link saveConfig → handleUsers:\n{he}");

    // The union has both edges.
    let ae = out.split("? all_edge").nth(1).unwrap_or("");
    assert!(ae.contains("saveConfig") && ae.contains("handleUsers"),
        "all_edge must contain both nodes:\n{ae}");

    // SCC finds the 2-member cycle.
    let lc = out.split("? loop_cycle").nth(1).unwrap_or("");
    assert!(lc.contains("(2 rows)"),
        "exactly 2 members in the loopback cycle:\n{lc}");
    assert!(lc.contains("saveConfig") && lc.contains("handleUsers"),
        "cycle contains both functions:\n{lc}");
}

/// A deeper chain: handler A calls B calls fetch→A. Same cycle, one more hop,
/// proving the virtual edge composes with multi-hop real call chains.
#[test]
fn http_loopback_with_intermediary() {
    let d = sandbox("deep");
    fs::write(d.join("src/app.ts"),
        "function listUsers() {\n\
         \x20\x20\x20\x20validate();\n\
         }\n\
         function validate() {\n\
         \x20\x20\x20\x20fetch(\"/api/list\");\n\
         }\n\
         app.get(\"/api/list\", listUsers);\n").unwrap();

    let prog = r#"
rel fns(sym: text).
fns(sym) <- type_entity(_, sym, _, _, _, _, _).

rel route(url: text, handler: text).
route(url, handler) <- scan("WORK", "src/**/*.ts", p, rev), match(p, rev, /app\.get\("(?<url>[^"]+)", (?<handler>\w+)\)/, l).

rel fetch_line(file: file, line: int, url: text).
fetch_line(p, l, url) <- scan("WORK", "src/**/*.ts", p, rev), match(p, rev, /fetch\("(?<url>[^"]+)"\)/, l).
rel fetch_edge(caller: text, url: text).
fetch_edge(caller, url) <- call_site(_, caller, "fetch", p, l), fetch_line(p, l, url).

rel http_edge(caller: text, handler: text).
http_edge(caller, handler_sym) <- fetch_edge(caller, url), route(url, handler_name), type_entity(_, handler_sym, handler_name, _, _, _, _).

rel all_edge(a: text, b: text).
all_edge(caller, callee) <- call_edge(caller, callee, _).
all_edge(caller, handler) <- http_edge(caller, handler).

rel cyc(rep: text, member: text).
cyc(rep, member) <- scc(all_edge).
rel cyc_size(rep: text, n: int).
cyc_size(rep, count(member)) <- cyc(rep, member).
rel loop_cycle(sym: text, size: int).
loop_cycle(member, n) <- cyc(rep, member), cyc_size(rep, n), n > 1.

? all_edge(a, b).
? loop_cycle(sym, size).
"#;
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "dl failed:\n{err}");

    let lc = out.split("? loop_cycle").nth(1).unwrap_or("");
    assert!(lc.contains("(2 rows)"),
        "2-member cycle through the intermediary:\n{lc}");
    assert!(lc.contains("listUsers") && lc.contains("validate"),
        "cycle spans both functions:\n{lc}");
}
