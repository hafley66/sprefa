use super::*;

/// The dl LSP VSCode extension, embedded so a prebuilt `dl` can install it with
/// no source tree (mirrors the SKILL_MD / starter embedding). The filename is
/// FIXED (`dl-lsp.vsix`, no version) so this line never changes per release; the
/// extension is same-versioned with the binary because `scripts/build-vsix.sh`
/// stamps the extension's package.json to the crate version and rebuilds this
/// artifact, and `build.rs` refuses to compile if the two drift. The committed
/// VSIX is the install artifact for a prebuilt `dl`.
const VSCODE_VSIX: &[u8] = include_bytes!("../../editors/vscode-dl/dl-lsp.vsix");
const VSCODE_VSIX_NAME: &str = concat!("dl-lsp-", env!("CARGO_PKG_VERSION"), ".vsix");

/// `dl setup --vscode`: install the dl LSP VSCode extension via the `code` CLI.
/// The extension starts `dl --lsp` with the workspace folder as cwd and proxies LSP over
/// stdio, so `.dl/*.dl` rules give live squiggles. Turnkey both ways: from a
/// repo checkout it builds a FRESH VSIX from `editors/vscode-dl` (always current
/// — no "did I rebuild the vsix?" step); a prebuilt `dl` run outside the source
/// falls back to the VSIX embedded at build time. Either path installs
/// uninstall-first to dodge the same-version reinstall no-op. No-op with a clear
/// message if `code` is not on PATH.
pub(super) fn install_vscode_extension() -> Result<i32> {
    if std::env::var_os("SPREFA_SETUP_NO_VSCODE").is_some() {
        println!("[dl setup] VSCode installation skipped (SPREFA_SETUP_NO_VSCODE).");
        return Ok(0);
    }
        let mut journal = SetupJournal::load()?;
        // Prefer a fresh build from the checked-out source; fall back to embedded.
        let (vsix, from_source) = match build_vscode_vsix(&mut journal) {
            Some(built) => (built, true),
            None => {
                let embedded = std::env::temp_dir().join(format!(
                    "{}-{}.vsix", VSCODE_VSIX_NAME.trim_end_matches(".vsix"), std::process::id()
                ));
                journal.stage_file(&embedded, VSCODE_VSIX)
                    .with_context(|| format!("write {}", embedded.display()))?;
                (embedded, false)
            }
        };
        // The same-version trap: `code --install-extension --force` silently no-ops
        // on a same-version reinstall while VSCode is running (a fresh build carries
        // the same version). Uninstall first so the new bits always land.
        let _ = std::process::Command::new("code")
            .args(["--uninstall-extension", "sprefa.dl-lsp"])
            .status();
        let status = std::process::Command::new("code")
            .arg("--install-extension")
            .arg(&vsix)
            .arg("--force")
            .status();
        match status {
            Ok(s) if s.success() => {
                let how = if from_source {
                    "freshly built from editors/vscode-dl"
                } else {
                    "embedded"
                };
                println!("[dl setup] installed the dl LSP VSCode extension ({how}).");
                println!("[dl setup] reload VSCode; open any file your .dl rules scan for live squiggles.");
                journal.finish_staged(&vsix)?;
                journal.save()?;
                Ok(0)
            }
            Ok(s) => {
                eprintln!(
                    "[dl setup] `code --install-extension` exited {s}; VSIX left at {}",
                    vsix.display()
                ); // @eprintln-ok: human-facing report of external command failure
                journal.save()?;
                Ok(1)
            }
            Err(_) => {
                println!(
                    "[dl setup] VSCode `code` CLI not found on PATH. The extension VSIX is at:"
                );
                println!("             {}", vsix.display());
                println!(
                    "[dl setup] install it by hand: `code --install-extension {}`",
                    vsix.display()
                );
                println!("[dl setup] (in VSCode, enable the `code` command via Shell Command: Install 'code' in PATH).");
                journal.save()?;
                Ok(0)
            }
    }
}

/// Build a fresh VSIX from the checked-out extension source when the tree and
/// toolchain are present (`editors/vscode-dl` under CWD + `npm` + `npx vsce`).
/// Returns the built VSIX path, or `None` to fall back to the embedded artifact
/// (a prebuilt `dl` run outside the repo, or any build step failing). `npm ci`
/// only when `node_modules` is missing, so a repeat install just compiles.
fn build_vscode_vsix(journal: &mut SetupJournal) -> Option<PathBuf> {
    let src = Path::new("editors/vscode-dl");
    if !src.join("package.json").exists() {
        return None;
    }
    let run = |args: &[&str]| -> bool {
        std::process::Command::new(args[0])
            .args(&args[1..])
            .current_dir(src)
            .status()
            .map(|st| st.success())
            .unwrap_or(false)
    };
    tracing::debug!("[dl setup] building the extension from {} ...", src.display());
    if !src.join("node_modules").exists() && !run(&["npm", "ci"]) && !run(&["npm", "install"]) {
        tracing::warn!("[dl setup] npm install failed; using the embedded VSIX instead");
        return None;
    }
    if !run(&["npm", "run", "compile"]) {
        tracing::warn!("[dl setup] extension compile failed; using the embedded VSIX instead");
        return None;
    }
    let out = std::env::temp_dir().join(format!("dl-lsp-fresh-{}.vsix", std::process::id()));
    let packaged = std::process::Command::new("npx")
        .args(["vsce", "package", "-o"])
        .arg(&out)
        .current_dir(src)
        .status()
        .map(|st| st.success())
        .unwrap_or(false);
    if packaged && out.exists() {
        if journal.record_staged(&out).is_err() { return None; }
        Some(out)
    } else {
        tracing::warn!("[dl setup] `vsce package` failed; using the embedded VSIX instead");
        None
    }
}
