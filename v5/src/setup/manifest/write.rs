use super::*;

impl SetupJournal {
    fn parent(&mut self, root: Option<&Path>, path: &Path) -> Result<()> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        let mut missing = Vec::new();
        let mut cursor = parent;
        while !cursor.exists() {
            missing.push(cursor.to_path_buf());
            cursor = cursor.parent().context("path has no existing ancestor")?;
        }
        for dir in missing.iter().rev() {
            fs::create_dir(dir)?;
            self.entry(root, dir, SetupKind::DirCreate, SetupDetail::DirCreate);
        }
        Ok(())
    }
    pub fn create_file(
        &mut self,
        root: Option<&Path>,
        target: &Path,
        bytes: &[u8],
    ) -> Result<bool> {
        safe(target, root)?;
        match fs::read(target) {
            Ok(old) if old == bytes => {
                self.entry(
                    root,
                    target,
                    SetupKind::FileCreate,
                    SetupDetail::FileCreate {
                        content_blake3: hash(bytes),
                    },
                );
                return Ok(false);
            }
            Ok(_) => {
                println!(
                    "[dl setup] exists, not ours, left alone: {}",
                    target.display()
                );
                return Ok(false);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.parent(root, target)?;
        atomic(target, bytes)?;
        self.entry(
            root,
            target,
            SetupKind::FileCreate,
            SetupDetail::FileCreate {
                content_blake3: hash(bytes),
            },
        );
        Ok(true)
    }
    pub fn append_marked(
        &mut self,
        root: Option<&Path>,
        target: &Path,
        body: &str,
        begin: &str,
        end: &str,
    ) -> Result<bool> {
        safe(target, root)?;
        let created = !target.exists();
        let old = fs::read_to_string(target).unwrap_or_default();
        if old.contains(begin) {
            let one_pair = old.matches(begin).count() == 1
                && old.matches(end).count() == 1
                && old
                    .find(begin)
                    .zip(old.find(end))
                    .is_some_and(|(a, z)| a < z);
            if !one_pair {
                println!(
                    "[dl setup] marker tampered, left alone: {}",
                    target.display()
                );
            } else {
                self.entry(
                    root,
                    target,
                    SetupKind::MarkedAppend,
                    SetupDetail::MarkedAppend {
                        begin_marker: begin.into(),
                        end_marker: end.into(),
                        created: false,
                    },
                );
            }
            return Ok(false);
        }
        let mut new = old;
        new.push_str(body);
        self.parent(root, target)?;
        atomic(target, new.as_bytes())?;
        self.entry(
            root,
            target,
            SetupKind::MarkedAppend,
            SetupDetail::MarkedAppend {
                begin_marker: begin.into(),
                end_marker: end.into(),
                created,
            },
        );
        Ok(true)
    }
    pub fn merge_json(
        &mut self,
        root: Option<&Path>,
        target: &Path,
        pointer: &str,
        added: Value,
    ) -> Result<bool> {
        safe(target, root)?;
        let text = match fs::read_to_string(target) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".into(),
            Err(e) => return Err(e.into()),
        };
        let mut value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "[dl setup] malformed JSON, left alone: {}",
                    target.display()
                );
                return Ok(false);
            }
        };
        let Some(arr) = json_array_mut(&mut value, pointer) else {
            println!(
                "[dl setup] JSON node is not an array, left alone: {}",
                target.display()
            );
            return Ok(false);
        };
        if arr.contains(&added) {
            self.entry(
                root,
                target,
                SetupKind::JsonMerge,
                SetupDetail::JsonMerge {
                    pointer: pointer.into(),
                    added,
                },
            );
            return Ok(false);
        }
        let keys: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
        for depth in 0..keys.len() {
            let prefix = format!("/{}", keys[..=depth].join("/"));
            if !json_edit::has_pointer(&text, &prefix) {
                let kind = if depth + 1 == keys.len() {
                    "__created_array__:"
                } else {
                    "__created_object__:"
                };
                self.entry(
                    root,
                    target,
                    SetupKind::JsonMerge,
                    SetupDetail::JsonMerge {
                        pointer: format!("{kind}{prefix}"),
                        added: Value::Null,
                    },
                );
            }
        }
        let output = json_edit::append_array_value(&text, pointer, &added)
            .context("preserve JSON array bytes")?;
        self.parent(root, target)?;
        atomic(target, output.as_bytes())?;
        self.entry(
            root,
            target,
            SetupKind::JsonMerge,
            SetupDetail::JsonMerge {
                pointer: pointer.into(),
                added,
            },
        );
        Ok(true)
    }
    pub fn symlink(
        &mut self,
        root: Option<&Path>,
        target: &Path,
        points_to: &Path,
    ) -> Result<bool> {
        safe(target, root)?;
        if fs::symlink_metadata(target).is_ok() {
            if fs::read_link(target).ok().as_deref() == Some(points_to) {
                self.entry(
                    root,
                    target,
                    SetupKind::Symlink,
                    SetupDetail::Symlink {
                        points_to: points_to.into(),
                    },
                );
                return Ok(false);
            }
            println!(
                "[dl setup] exists, not ours, left alone: {}",
                target.display()
            );
            return Ok(false);
        }
        self.parent(root, target)?;
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(points_to, target)?;
        }
        #[cfg(not(unix))]
        {
            fs::copy(points_to, target)?;
        }
        self.entry(
            root,
            target,
            SetupKind::Symlink,
            SetupDetail::Symlink {
                points_to: points_to.into(),
            },
        );
        Ok(true)
    }
    pub fn hook_config(
        &mut self,
        root: &Path,
        target: &Path,
        value: &Value,
        added: &[(String, String)],
    ) -> Result<()> {
        safe(target, Some(root))?;
        let created = !target.exists();
        let original = if created {
            None
        } else {
            Some(fs::read_to_string(target)?)
        };
        let hooks_existed = original
            .as_deref()
            .is_some_and(|text| json_edit::has_pointer(text, "/hooks"));
        let created_events: Vec<String> = original
            .as_deref()
            .map(|text| {
                added
                    .iter()
                    .filter(|(event, _)| !json_edit::has_pointer(text, &format!("/hooks/{event}")))
                    .map(|(event, _)| event.clone())
                    .collect()
            })
            .unwrap_or_default();
        self.parent(Some(root), target)?;
        if created {
            atomic(target, &serde_json::to_vec_pretty(value)?)?;
        } else {
            let mut output = fs::read_to_string(target)?;
            for (event, command) in added {
                let entry = value
                    .get("hooks")
                    .and_then(|hooks| hooks.get(event))
                    .and_then(Value::as_array)
                    .and_then(|entries| {
                        entries.iter().find(|entry| {
                            entry
                                .get("hooks")
                                .and_then(Value::as_array)
                                .is_some_and(|hooks| {
                                    hooks.iter().any(|hook| {
                                        hook.get("command")
                                            .and_then(Value::as_str)
                                            .is_some_and(|value| value.contains(command))
                                    })
                                })
                        })
                    })
                    .context("added hook entry missing")?;
                output = json_edit::append_array_value(&output, &format!("/hooks/{event}"), entry)
                    .context("preserve hook JSON bytes")?;
            }
            atomic(target, output.as_bytes())?;
        }
        if !created && !hooks_existed {
            self.entry(
                Some(root),
                target,
                SetupKind::HookRegister,
                SetupDetail::HookRegister {
                    event: "__created_object__:hooks".into(),
                    command_substring: "dl setup".into(),
                },
            );
        }
        for event in created_events {
            self.entry(
                Some(root),
                target,
                SetupKind::HookRegister,
                SetupDetail::HookRegister {
                    event: format!("__created_event__:{event}"),
                    command_substring: "dl setup".into(),
                },
            );
        }
        if created {
            self.entry(
                Some(root),
                target,
                SetupKind::HookRegister,
                SetupDetail::HookRegister {
                    event: "__created_file__".into(),
                    command_substring: "dl setup".into(),
                },
            );
        }
        for (event, command) in added {
            self.entry(
                Some(root),
                target,
                SetupKind::HookRegister,
                SetupDetail::HookRegister {
                    event: event.clone(),
                    command_substring: command.clone(),
                },
            );
        }
        Ok(())
    }
    pub fn record_hook(&mut self, root: &Path, target: &Path, event: &str, command: &str) {
        self.entry(
            Some(root),
            target,
            SetupKind::HookRegister,
            SetupDetail::HookRegister {
                event: event.into(),
                command_substring: command.into(),
            },
        );
    }
    pub fn git_hooks_path(&mut self, root: &Path) -> Result<bool> {
        let target = root.join(".git/config");
        safe(&target, Some(root))?;
        let current = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "--get", "core.hooksPath"])
            .output()?;
        let value = String::from_utf8_lossy(&current.stdout).trim().to_string();
        if value == ".githooks" {
            self.record_hook(root, &target, "core.hooksPath", ".githooks");
            return Ok(false);
        }
        if !value.is_empty() {
            println!("[dl setup] core.hooksPath exists, not ours, left alone: {value}");
            return Ok(false);
        }
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "core.hooksPath", ".githooks"])
            .status()?;
        if !status.success() {
            anyhow::bail!("git config core.hooksPath failed")
        }
        self.record_hook(root, &target, "core.hooksPath", ".githooks");
        Ok(true)
    }
    #[cfg(unix)]
    pub fn make_executable(&self, target: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }
}
