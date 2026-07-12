use super::*;

impl SetupJournal {
    pub fn undo(&mut self, root: Option<&Path>, global_only: bool, dry: bool) -> Result<i32> {
        let mut kept = Vec::new();
        for entry in self.entries.iter().rev() {
            if (global_only && entry.root.is_some())
                || root.is_some_and(|r| entry.root.as_deref() != Some(r)) {
                kept.push(entry.clone());
                continue;
            }
            if !safe(&entry.target, entry.root.as_deref()).is_ok() {
                println!("[dl setup] SKIP unsafe path: {}", entry.target.display());
                kept.push(entry.clone());
                continue;
            }
            if matches!(fs::symlink_metadata(&entry.target), Err(error) if error.kind() == std::io::ErrorKind::NotFound) {
                println!("[dl setup] already absent {}", entry.target.display());
                continue;
            }
            let action = match &entry.detail {
                SetupDetail::FileCreate { content_blake3 } => match fs::read(&entry.target) {
                    Ok(b) if hash(&b) == *content_blake3 => Some("remove"),
                    Ok(_) => {
                        println!(
                            "[dl setup] modified since install, left in place: {}",
                            entry.target.display()
                        );
                        None
                    }
                    Err(_) => None,
                },
                SetupDetail::Symlink { points_to } => match fs::read_link(&entry.target) {
                    Ok(p) if p == *points_to => Some("remove"),
                    _ => {
                        println!(
                            "[dl setup] symlink changed, left in place: {}",
                            entry.target.display()
                        );
                        None
                    }
                },
                SetupDetail::MarkedAppend {
                    begin_marker,
                    end_marker,
                } => match fs::read_to_string(&entry.target) {
                    Ok(s) if marker_range(&s, begin_marker, end_marker).is_some() => {
                        Some("strip markers")
                    }
                    _ => {
                        println!(
                            "[dl setup] marker changed, left in place: {}",
                            entry.target.display()
                        );
                        None
                    }
                },
                SetupDetail::JsonMerge { pointer, added } => {
                    let removed = if let Some(created) = pointer.strip_prefix("__created_array__:") {
                        remove_created_json_path(&entry.target, created, "[]", dry)?
                    } else if let Some(created) = pointer.strip_prefix("__created_object__:") {
                        remove_created_json_path(&entry.target, created, "{}", dry)?
                    } else {
                        remove_json(&entry.target, pointer, added, dry)?
                    };
                    if removed {
                        Some("remove JSON node")
                    } else {
                        println!("[dl setup] JSON node changed, left in place: {}", entry.target.display());
                        None
                    }
                }
                SetupDetail::HookRegister {
                    event,
                    command_substring,
                } => {
                    let removed = if event == "__created_file__" {
                        remove_empty_hook_file(&entry.target, dry)?
                    } else if let Some(created_event) = event.strip_prefix("__created_event__:") {
                        remove_empty_json_member(&entry.target, "/hooks", created_event, "[]", dry)?
                    } else if event == "__created_object__:hooks" {
                        remove_empty_json_member(&entry.target, "", "hooks", "{}", dry)?
                    } else if event == "core.hooksPath" {
                        remove_git_hook_path(entry.root.as_deref(), command_substring, dry)?
                    } else {
                        remove_hook(&entry.target, event, command_substring, dry)?
                    };
                    if removed {
                        Some("remove hook")
                    } else {
                        println!(
                            "[dl setup] hook changed, left in place: {}",
                            entry.target.display()
                        );
                        None
                    }
                }
            };
            if let Some(action) = action {
                println!("[dl setup] {} {}", action, entry.target.display());
                if !dry {
                    match &entry.detail {
                        SetupDetail::MarkedAppend {
                            begin_marker,
                            end_marker,
                        } => {
                            let s = fs::read_to_string(&entry.target)?;
                            let Some((a, z)) = marker_range(&s, begin_marker, end_marker) else {
                                println!(
                                    "[dl setup] marker changed, left in place: {}",
                                    entry.target.display()
                                );
                                kept.push(entry.clone());
                                continue;
                            };
                            atomic(&entry.target, format!("{}{}", &s[..a], &s[z..]).as_bytes())?
                        }
                        SetupDetail::JsonMerge { .. } => {}
                        SetupDetail::HookRegister { event, .. } if event != "__created_file__" => {}
                        SetupDetail::HookRegister { .. } => {
                            if entry.target.exists() { fs::remove_file(&entry.target)?; }
                        }
                        _ => {
                            fs::remove_file(&entry.target)?;
                        }
                    }
                }
            } else {
                kept.push(entry.clone());
            }
        }
        kept.reverse();
        self.entries = kept;
        if !dry {
            self.save()?;
        }
        Ok(0)
    }
    pub fn adopt(&mut self) -> Result<i32> {
        let root = std::env::current_dir()?.canonicalize()?;
        let mut adopted = 0;
        for file in [root.join("AGENTS.md"), root.join("CLAUDE.md")] {
            match fs::read_to_string(&file) {
                Ok(text)
                    if marker_range(&text, "<!-- BEGIN: sprefa-dl -->", "<!-- END: sprefa-dl -->").is_some() =>
                {
                    self.entry(
                        Some(&root),
                        &file,
                        SetupKind::MarkedAppend,
                        SetupDetail::MarkedAppend {
                            begin_marker: "<!-- BEGIN: sprefa-dl -->".into(),
                            end_marker: "<!-- END: sprefa-dl -->".into(),
                        },
                    );
                    println!("[dl setup] adopted markers: {}", file.display());
                    adopted += 1;
                }
                Ok(text) if text.contains("<!-- BEGIN: sprefa-dl -->") => {
                    println!("[dl setup] declined tampered markers: {}", file.display())
                }
                _ => {}
            }
        }
        for (file, command) in [
            (root.join(".claude/settings.json"), "dl --hook"),
            (root.join(".codex/hooks.json"), "dl --hook --dialect codex"),
        ] {
            match fs::read_to_string(&file)
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            {
                Some(value) => {
                    for event in ["PostToolUse", "UserPromptSubmit"] {
                        if has_hook(&value, event, command) {
                            self.entry(
                                Some(&root),
                                &file,
                                SetupKind::HookRegister,
                                SetupDetail::HookRegister {
                                    event: event.into(),
                                    command_substring: command.into(),
                                },
                            );
                            println!("[dl setup] adopted hook: {} {}", file.display(), event);
                            adopted += 1;
                        }
                    }
                }
                None if file.exists() => {
                    println!("[dl setup] declined malformed JSON: {}", file.display())
                }
                _ => {}
            }
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut links = Vec::new();
        if let Ok(skills) = fs::read_dir(root.join(".claude/skills")) {
            links.extend(skills.flatten().map(|entry| entry.path().join("SKILL.md")));
        }
        if let Some(home) = &home {
            links.push(home.join(".claude/skills/sprefa-dl/SKILL.md"));
        }
        for link in links {
            if let Ok(points_to) = fs::read_link(&link) {
                if !verified_skill_link(&root, home.as_deref(), &link, &points_to) {
                    println!("[dl setup] declined unrecognized symlink: {}", link.display());
                    continue;
                }
                let owner = if link.starts_with(&root) {
                    Some(root.as_path())
                } else {
                    None
                };
                self.entry(
                    owner,
                    &link,
                    SetupKind::Symlink,
                    SetupDetail::Symlink { points_to },
                );
                println!("[dl setup] adopted symlink: {}", link.display());
                adopted += 1;
            }
        }
        self.save()?;
        println!("[dl setup] adopted {adopted} verified entries");
        Ok(0)
    }
}
fn marker_range(text: &str, begin: &str, end: &str) -> Option<(usize, usize)> {
    if text.matches(begin).count() != 1 || text.matches(end).count() != 1 { return None; }
    let start = text.find(begin)?;
    let end_start = text.find(end)?;
    (start < end_start).then_some((start, end_start + end.len()))
}
fn remove_empty_hook_file(target: &Path, _dry: bool) -> Result<bool> {
    let value: Value = match fs::read_to_string(target).ok().and_then(|text| serde_json::from_str(&text).ok()) {
        Some(value) => value,
        None => return Ok(false),
    };
    let Some(object) = value.as_object() else { return Ok(false) };
    if object.len() != 1 { return Ok(false); }
    let Some(hooks) = object.get("hooks").and_then(Value::as_object) else { return Ok(false) };
    Ok(hooks.values().all(|events| events.as_array().is_some_and(Vec::is_empty)))
}
fn remove_empty_json_member(target: &Path, parent: &str, key: &str, empty: &str, dry: bool) -> Result<bool> {
    let text = match fs::read_to_string(target) { Ok(text) => text, Err(_) => return Ok(false) };
    let Some(output) = json_edit::remove_empty_member(&text, parent, key, empty) else { return Ok(false) };
    if !dry { atomic(target, output.as_bytes())?; }
    Ok(true)
}
fn remove_created_json_path(target: &Path, pointer: &str, empty: &str, dry: bool) -> Result<bool> {
    let (parent, key) = pointer.rsplit_once('/').unwrap_or(("", pointer.trim_start_matches('/')));
    remove_empty_json_member(target, parent, key, empty, dry)
}
fn verified_skill_link(root: &Path, home: Option<&Path>, link: &Path, points_to: &Path) -> bool {
    let resolved = if points_to.is_absolute() { points_to.to_path_buf() }
        else { link.parent().unwrap_or(root).join(points_to) };
    let Ok(resolved) = resolved.canonicalize() else { return false };
    resolved.starts_with(root.join("assets"))
        || resolved.starts_with(root.join(".agents/skills"))
        || home.is_some_and(|home| resolved.starts_with(home.join(".agents/skills")))
}
fn remove_git_hook_path(root: Option<&Path>, expected: &str, dry: bool) -> Result<bool> {
    let Some(root) = root else { return Ok(false) };
    let out = std::process::Command::new("git").arg("-C").arg(root)
        .args(["config", "--get", "core.hooksPath"]).output()?;
    if String::from_utf8_lossy(&out.stdout).trim() != expected { return Ok(false); }
    if !dry {
        let status = std::process::Command::new("git").arg("-C").arg(root)
            .args(["config", "--unset", "core.hooksPath"]).status()?;
        if !status.success() { return Ok(false); }
    }
    Ok(true)
}
fn remove_json(target: &Path, pointer: &str, added: &Value, dry: bool) -> Result<bool> {
    let s = match fs::read_to_string(target) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    let v: Value = match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(_) => {
            println!(
                "[dl setup] malformed JSON, left in place: {}",
                target.display()
            );
            return Ok(false);
        }
    };
    if !v.pointer(pointer).and_then(Value::as_array).is_some_and(|array| array.contains(added)) { return Ok(false); }
    if !dry {
        let output = json_edit::remove_array_value(&s, pointer, added).context("remove exact JSON node")?;
        atomic(target, output.as_bytes())?;
    }
    Ok(true)
}
