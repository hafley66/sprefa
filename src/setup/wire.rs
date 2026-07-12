use super::*;

// ── global: install the skill where the agents read it ────────────────────────
pub(crate) fn wire_global(skills_dir: Option<String>) -> Result<i32> {
    let mut journal = SetupJournal::load()?;
    let dest = resolve_skills_dir(skills_dir)?;
    let skill_dir = dest.join("sprefa-dl");
    journal.create_file(None, &skill_dir.join("SKILL.md"), SKILL_MD.as_bytes())?;
    println!(
        "[dl setup] skill -> {}",
        skill_dir.join("SKILL.md").display()
    );

    if let Ok(h) = home() {
        // ~/.agents/skills is the cross-harness home (codex and opencode read
        // it natively; dl's own resolve_skill checks it first). Write the
        // primary copy there whenever it is not already the dest.
        let agents = h.join(".agents/skills");
        if agents.canonicalize().ok() != dest.canonicalize().ok() {
            let ad = agents.join("sprefa-dl");
            if journal
                .create_file(None, &ad.join("SKILL.md"), SKILL_MD.as_bytes())
                .unwrap_or(false)
            {
                println!(
                    "[dl setup] cross-harness -> {}",
                    ad.join("SKILL.md").display()
                );
            }
        }
    }

    // Claude Code reads ~/.claude/skills. If that is not already the dest, drop a
    // copy there too (a copy, not a symlink — most portable across machines).
    // Detect Claude Code by ~/.claude (its config dir, present after first run);
    // the skills/ subdir may not exist yet on a fresh machine, so create it.
    if let Ok(h) = home() {
        let cc = h.join(".claude/skills");
        if h.join(".claude").is_dir() && cc.canonicalize().ok() != dest.canonicalize().ok() {
            let ccd = cc.join("sprefa-dl");
            if journal
                .create_file(None, &ccd.join("SKILL.md"), SKILL_MD.as_bytes())
                .unwrap_or(false)
            {
                println!(
                    "[dl setup] claude code -> {}",
                    ccd.join("SKILL.md").display()
                );
            }
        }
        // opencode: ensure skills.paths contains `dest`.
        wire_opencode(&h, &dest, &mut journal);
    }
    journal.save()?;
    println!("[dl setup] done. agents will pick up the skill on next launch.");
    Ok(0)
}

/// Refresh the on-disk artifacts that embed version-pinned content, so a
/// `dl update` that swapped in a new binary also updates what agents/editors
/// read off disk. Called from `dl update` after a successful install.
///
/// - **Skill (global):** re-writes `SKILL.md` from the new binary's embedded
///   copy at every location `dl setup` wrote it (the resolved skills dir, the
///   `~/.claude/skills` copy, and the opencode path). Idempotent overwrite.
/// - **VSCode extension:** reinstalled from the new embedded VSIX ONLY when the
///   user already has it (preserves their opt-in; never installs on a machine
///   that opted out).
///
/// Project-level wiring (`.claude/settings.json` hooks, `.githooks`, the
/// AGENTS/CLAUDE dl section, `.dl/` starters) lives in arbitrary repos, so it
/// cannot be refreshed from here without walking the filesystem; the caller
/// prints a hint to re-run `dl setup --project` in any bootstrapped repo. MCP
/// has no on-disk wiring (agents spawn `dl --mcp` fresh, picking up the new
/// binary automatically), so there is nothing to refresh there.
pub(crate) fn refresh_after_update() {
    println!("[dl update] refreshing the on-disk skill from the new binary...");
    match wire_global(None) {
        Ok(_) => {}
        Err(e) => eprintln!("[dl update] skill refresh failed ({e}); run `dl setup` by hand"),
    }
    if vscode_extension_installed() {
        println!("[dl update] reinstalling the dl LSP VSCode extension...");
        let _ = install_vscode_extension();
    }
}

/// Whether the dl LSP VSCode extension is currently installed (so `dl update`
/// reinstalls only for users who opted in). `code --list-extensions` prints one
/// id per line; we look for the extension's publisher.id. Returns false when
/// `code` is absent (not installed / not on PATH).
fn vscode_extension_installed() -> bool {
    let out = match std::process::Command::new("code")
        .arg("--list-extensions")
        .output()
    {
        Ok(o) => o,
        Err(_) => return false, // `code` CLI not present
    };
    out.status.success()
        && String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l == "sprefa.dl-lsp")
}

/// Pick the skill destination: explicit flag > $SPREFA_SKILLS_DIR > opencode's
/// first configured path > ~/.claude/skills > ~/.config/sprefa/skills.
fn resolve_skills_dir(flag: Option<String>) -> Result<PathBuf> {
    if let Some(d) = flag {
        return Ok(PathBuf::from(d));
    }
    if let Some(d) = std::env::var_os("SPREFA_SKILLS_DIR") {
        return Ok(PathBuf::from(d));
    }
    let h = home()?;
    if let Some(p) = opencode_first_skill_path(&h) {
        if p.is_dir() {
            return Ok(p);
        }
    }
    // Claude Code: prefer its skills dir whenever Claude Code is present
    // (~/.claude exists), even if the skills/ subdir hasn't been created yet on a
    // fresh machine — wire_global creates it. (The old `cc.is_dir()` check fell
    // through to the no-agent stash below, where Claude Code never reads it.)
    let cc = h.join(".claude/skills");
    if cc.is_dir() || h.join(".claude").is_dir() {
        return Ok(cc);
    }
    // No agent detected: stash under XDG. Warn, since nothing reads it until an
    // agent is pointed at it (SPREFA_SKILLS_DIR or opencode skills.paths).
    eprintln!(
        "[dl setup] no Claude Code (~/.claude) or opencode config found; \
               stashing the skill under ~/.config/sprefa/skills. Point an agent at \
               it with SPREFA_SKILLS_DIR=<dir> or `dl setup --skills-dir <dir>`."
    );
    Ok(h.join(".config/sprefa/skills"))
}

fn opencode_cfg(h: &Path) -> PathBuf {
    h.join(".config/opencode/opencode.json")
}

fn opencode_first_skill_path(h: &Path) -> Option<PathBuf> {
    let txt = std::fs::read_to_string(opencode_cfg(h)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let p = v
        .get("skills")?
        .get("paths")?
        .as_array()?
        .first()?
        .as_str()?;
    Some(PathBuf::from(p))
}

/// Add `dest` to opencode.json `skills.paths` if missing, preserving every other
/// key. Pretty-prints back. If the file is absent or unparseable, prints the
/// snippet for the user to add by hand.
fn wire_opencode(h: &Path, dest: &Path, journal: &mut SetupJournal) {
    let cfg = opencode_cfg(h);
    let dest_s = dest.to_string_lossy().into_owned();
    let Ok(txt) = std::fs::read_to_string(&cfg) else {
        return; // opencode not installed; nothing to wire
    };
    if serde_json::from_str::<serde_json::Value>(&txt).is_err() {
        println!(
            "[dl setup] add to {}: \"skills\": {{ \"paths\": [\"{dest_s}\"] }}",
            cfg.display()
        );
        return;
    }
    match journal.merge_json(None, &cfg, "/skills/paths", serde_json::Value::String(dest_s.clone())) {
        Ok(true) => println!("[dl setup] opencode skills.paths += {dest_s}"),
        Ok(false) => println!("[dl setup] opencode already reads {dest_s}"),
        Err(e) => println!("[dl setup] opencode left alone: {e}"),
    }
}
