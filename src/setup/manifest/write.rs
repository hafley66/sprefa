use super::*;

impl SetupJournal {
    pub fn create_file(
        &mut self,
        root: Option<&Path>,
        target: &Path,
        bytes: &[u8],
    ) -> Result<bool> {
        safe(target, root)?;
        match fs::read(target) {
            Ok(old) if old == bytes => {
                self.entry(root, target, SetupKind::FileCreate,
                    SetupDetail::FileCreate { content_blake3: hash(bytes) });
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
        parent(target)?;
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
        let old = fs::read_to_string(target).unwrap_or_default();
        if old.contains(begin) {
            let one_pair = old.matches(begin).count() == 1 && old.matches(end).count() == 1
                && old.find(begin).zip(old.find(end)).is_some_and(|(a, z)| a < z);
            if !one_pair {
                println!(
                    "[dl setup] marker tampered, left alone: {}",
                    target.display()
                );
            }
            else {
                self.entry(root, target, SetupKind::MarkedAppend,
                    SetupDetail::MarkedAppend { begin_marker: begin.into(), end_marker: end.into() });
            }
            return Ok(false);
        }
        let mut new = old;
        new.push_str(body);
        parent(target)?;
        atomic(target, new.as_bytes())?;
        self.entry(
            root,
            target,
            SetupKind::MarkedAppend,
            SetupDetail::MarkedAppend {
                begin_marker: begin.into(),
                end_marker: end.into(),
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
            return Ok(false);
        }
        arr.push(added.clone());
        parent(target)?;
        atomic(target, &serde_json::to_vec_pretty(&value)?)?;
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
                self.entry(root, target, SetupKind::Symlink,
                    SetupDetail::Symlink { points_to: points_to.into() });
                return Ok(false);
            }
            println!(
                "[dl setup] exists, not ours, left alone: {}",
                target.display()
            );
            return Ok(false);
        }
        parent(target)?;
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
        parent(target)?;
        atomic(target, &serde_json::to_vec_pretty(value)?)?;
        if created {
            self.entry(Some(root), target, SetupKind::HookRegister,
                SetupDetail::HookRegister { event: "__created_file__".into(), command_substring: "dl setup".into() });
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
        self.entry(Some(root), target, SetupKind::HookRegister,
            SetupDetail::HookRegister { event: event.into(), command_substring: command.into() });
    }
    pub fn git_hooks_path(&mut self, root: &Path) -> Result<bool> {
        let target = root.join(".git/config");
        safe(&target, Some(root))?;
        let current = std::process::Command::new("git").arg("-C").arg(root)
            .args(["config", "--get", "core.hooksPath"]).output()?;
        let value = String::from_utf8_lossy(&current.stdout).trim().to_string();
        if value == ".githooks" {
            self.record_hook(root, &target, "core.hooksPath", ".githooks");
            return Ok(false);
        }
        if !value.is_empty() {
            println!("[dl setup] core.hooksPath exists, not ours, left alone: {value}");
            return Ok(false);
        }
        let status = std::process::Command::new("git").arg("-C").arg(root)
            .args(["config", "core.hooksPath", ".githooks"]).status()?;
        if !status.success() { anyhow::bail!("git config core.hooksPath failed") }
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
