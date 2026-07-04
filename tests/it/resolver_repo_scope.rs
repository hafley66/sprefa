//! Regression for D5a (PR-diff-graph arc): the syntactic name-unique resolver
//! in `refresh_type_rels`/`refresh_call_rels` must be scoped to the ref site's
//! repo, and must not read the SAME physical file scanned under two slugs (a
//! config repo pointing at the self root, or two worktrees sharing a `.git`
//! basename) as an ambiguous double-definition.
//!
//! Setup: `alpha` is BOTH the `--root` (self) and a config repo, so its files
//! are scanned twice and both scans collapse to rid `alpha` via `repo_id_of`
//! (nearest-`.git` basename). Before the fix, `by_name[("alpha", "Widget")]`
//! held the same sym twice -> len 2 -> ambiguous -> every alpha ref resolved
//! bare, while single-scanned `beta` resolved fine (the corpus-flat symptom).
//! After the fix, the ambiguity bucket dedups by def sym, so alpha resolves
//! in-repo again.

use std::fs;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git").current_dir(dir).args(args).output().expect("git").status.success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

/// A repo dir with a `.git` (so `repo_id_of` reads its basename as the rid) and
/// one Rust file declaring `Gadget`, `Widget { part: Gadget }`, and a
/// repo-unique `{Prefix}Only { w: Widget }`.
fn make_repo(root: &std::path::Path, unique: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        format!(
            "pub struct Gadget {{ pub n: i64 }}\n\
             pub struct Widget {{ pub part: Gadget }}\n\
             pub struct {unique}Only {{ pub w: Widget }}\n"
        ),
    )
    .unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@t"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "x"]);
}

#[test]
fn resolver_scopes_candidates_to_the_ref_site_repo() {
    let d = std::env::temp_dir().join("resolver_repo_scope_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    // `alpha` shares the colliding names Widget/Gadget with `beta`. alpha is
    // the self root AND a config repo (double-scanned, one rid). beta is a
    // config-only single-scanned control.
    let alpha = d.join("alpha");
    let beta = d.join("beta");
    make_repo(&alpha, "Alpha");
    make_repo(&beta, "Beta");

    // alpha-cfg points a SECOND slug at the alpha root, so alpha's files land in
    // `_file` under both the self slug and "alpha-cfg" -- the double scan.
    fs::write(
        d.join("cfg.toml"),
        format!(
            "[[repos]]\n\
             slug = \"alpha-cfg\"\n\
             root = \"{alpha}\"\n\
             [[repos]]\n\
             slug = \"beta\"\n\
             root = \"{beta}\"\n",
            alpha = alpha.display(),
            beta = beta.display(),
        ),
    )
    .unwrap();

    // Scan the self root AND fan over the config repos, then print type_link.
    // Referencing type_link opts type extraction in over the whole file set.
    fs::write(
        d.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n\
         seen(path) <- scan(\"*\", \"WORK\", \"src/**/*.rs\", path, rev).\n\
         ? type_link(src, dst, kind).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", alpha.to_str().unwrap(), "--no-daemon", "--db", d.join("db").to_str().unwrap()])
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "run failed: {stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    let a_widget = "alpha::src/lib.rs::struct::Widget";
    let a_gadget = "alpha::src/lib.rs::struct::Gadget";
    let a_only = "alpha::src/lib.rs::struct::AlphaOnly";
    let b_widget = "beta::src/lib.rs::struct::Widget";
    let b_gadget = "beta::src/lib.rs::struct::Gadget";
    let b_only = "beta::src/lib.rs::struct::BetaOnly";

    // (a) each repo's colliding ref resolves to its OWN repo's def. The alpha
    // edge is the discriminator: without the dedup fix alpha is ambiguous and
    // this edge is bare/absent.
    assert!(
        stdout.contains(&format!("{a_widget}\t{a_gadget}\tfield")),
        "alpha Widget->Gadget resolves in alpha (the double-scan fix):\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("{b_widget}\t{b_gadget}\tfield")),
        "beta Widget->Gadget resolves in beta:\n{stdout}"
    );

    // (b) a name declared in only one repo still resolves for a ref in that repo.
    assert!(
        stdout.contains(&format!("{a_only}\t{a_widget}\tfield")),
        "alpha AlphaOnly->Widget resolves (repo-unique owner, colliding target):\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("{b_only}\t{b_widget}\tfield")),
        "beta BetaOnly->Widget resolves:\n{stdout}"
    );

    // (c) a cross-repo-colliding name never produces a cross-repo edge.
    assert!(
        !stdout.contains(&format!("{a_widget}\t{b_gadget}")),
        "no alpha->beta cross edge:\n{stdout}"
    );
    assert!(
        !stdout.contains(&format!("{b_widget}\t{a_gadget}")),
        "no beta->alpha cross edge:\n{stdout}"
    );
    assert!(
        !stdout.contains(&format!("{a_only}\t{b_widget}")),
        "no alpha owner -> beta target cross edge:\n{stdout}"
    );
}
