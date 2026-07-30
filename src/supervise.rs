//! OS service-manager supervision (plans/2026-07-18-infra-library-adoption.md
//! section 3; standing law a1f049ff "infra is bought, never built"): the
//! `service-manager` crate owns install/uninstall (writing a hand-authored
//! plist/unit through `ServiceInstallCtx.contents`, per section 3.1/3.3); thin
//! `launchctl`/`systemctl` wrappers here own start/stop/restart/status per the
//! plan's CLI mapping (bootstrap+kickstart / bootout / kickstart -k), since the
//! crate's own start/stop use an older verb family (`start`/`stop`/`load`/
//! `remove`) that doesn't match the plan's chosen shape.
//!
//! macOS (launchd) is the primary, exercised path. Linux (systemd) is written
//! to the same shape per section 3.2 but is `cfg(target_os = "linux")`-gated
//! and untested on this machine (task-scoped: "stub if untestable here").
//!
//! What this module deliberately does NOT own: daemonization, respawn-on-crash,
//! log rotation, or resource-tier scheduling once installed — those are
//! `launchd`/`systemd`'s job via the plist/unit keys below (`KeepAlive`,
//! `Nice`, `LowPriorityIO`). `apply_daemon_budget` in `crate::daemon` still
//! runs the in-process CPU-percent governor and thread caps (plan section 3.5
//! point 3: no OS mechanism on macOS gives a CPU-percent cap) and skips the
//! now-redundant nice/IOPOL/QoS calls when it detects `XPC_SERVICE_NAME`
//! (launchd sets this on every process it spawns — confirmed via
//! `launchctl print` during the gate-2 probe below).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The service's label everywhere the OS-native identifier is needed:
/// `~/Library/LaunchAgents/com.sprefa.dl.plist` on macOS,
/// `~/.config/systemd/user/dl.service` on Linux (label -> unit filename is
/// `dl` there; see `systemd_unit_path`).
const LABEL_QUALIFIER: &str = "com";
const LABEL_ORG: &str = "sprefa";
const LABEL_APP: &str = "dl";

pub fn service_label() -> service_manager::ServiceLabel {
    service_manager::ServiceLabel {
        qualifier: Some(LABEL_QUALIFIER.to_string()),
        organization: Some(LABEL_ORG.to_string()),
        application: LABEL_APP.to_string(),
    }
}

fn qualified_label() -> String {
    service_label().to_qualified_name()
}

/// `~/Library/LaunchAgents/com.sprefa.dl.plist` — computed the same way the
/// `service-manager` crate computes it internally (`dirs::home_dir()`), so
/// `is_installed`/the launchctl wrappers agree with whatever `install()` wrote.
#[cfg(target_os = "macos")]
fn plist_path() -> Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", qualified_label())))
}

/// `$HOME`, resolved the same minimal way `crate::daemon::daemon_home`'s
/// fallback does — no `dirs` crate direct dependency needed for one lookup
/// (`service-manager` already pulls it in transitively and uses it
/// identically inside `LaunchdServiceManager::get_plist_path`, so this agrees
/// with wherever the crate itself wrote the plist).
fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

/// `gui/<uid>` — the launchd user-session target every `launchctl bootstrap/
/// kickstart/bootout/print` call below addresses (never the system/root
/// domain: this is a user-level LaunchAgent, no elevated privileges needed or
/// wanted).
#[cfg(target_os = "macos")]
fn gui_domain() -> String {
    // SAFETY: getuid() takes no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };
    format!("gui/{uid}")
}

#[cfg(target_os = "macos")]
fn domain_target() -> String {
    format!("{}/{}", gui_domain(), qualified_label())
}

/// Author the LaunchAgent plist. `KeepAlive.SuccessfulExit: false` means a
/// clean `dl daemon stop` (graceful shutdown RPC, exit 0) stays stopped;
/// `Crashed: true` restarts on a crash signal — the OR of the two conditions
/// launchd documents (launchd.plist(5)). `Nice`/`LowPriorityIO` replace the
/// in-process `nice()`/`setiopolicy_np()` calls this arc deletes from
/// `crate::daemon::apply_daemon_budget` when it detects `XPC_SERVICE_NAME`.
/// Deliberately NOT `ProcessType: Background` — the gate-2 probe (plan section
/// 5.2 q2) measured that tier at ~45% of baseline CPU throughput (two 5s
/// fixed-duration spinner trials, ~2.2x wall-clock cost), and the in-process
/// CPU-percent governor (kept per plan section 3.5 point 3) already bounds
/// total consumption — stacking the hard efficiency-core clamp on top buys no
/// extra safety for a real cost to first-run rebuild latency.
/// `RunAtLoad` is omitted (defaults false): install never auto-starts,
/// consistent with the `service-manager` crate's own cross-platform
/// convention (see its `Disabled`-flag handling) — `dl daemon start` is the
/// explicit trigger.
#[cfg(target_os = "macos")]
pub fn plist_contents(program: &Path, home: &Path) -> String {
    let label = qualified_label();
    // Named path fns (`crate::daemon::launchd_std{out,err}_log_path`), not an
    // inline `.join`, so the plist generator and the log-cap sweep
    // (`daemon::logcap`, failure-modes class 31) can never drift apart on
    // where these files live.
    let stdout = crate::daemon::launchd_stdout_log_path(home);
    let stderr = crate::daemon::launchd_stderr_log_path(home);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
        <string>daemon</string>
        <string>serve</string>
    </array>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>Nice</key>
    <integer>10</integer>
    <key>LowPriorityIO</key>
    <true/>
    <key>ExitTimeOut</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
</dict>
</plist>
"#,
        program = program.display(),
        stdout = stdout.display(),
        stderr = stderr.display(),
    )
}

/// `~/.config/systemd/user/dl.service` — plan section 3.2's parallel path.
/// `Type=notify` + `sd-notify` (the daemon's own `notify(Ready)` call, wired
/// in `crate::daemon::run_daemon` once the UDS listener is bound) gives
/// systemd a real readiness signal launchd has no equivalent for.
/// `CPUQuota`/`IOWeight` are commented out per the plan's finding (section
/// 3.2): most distros delegate only memory+pids to `user@.service` by
/// default, so a hard CPU percent needs a root-installed `Delegate=` drop-in
/// this installer cannot assume exists. `Nice`/`IOSchedulingClass` work
/// without delegation and mirror the launchd `Nice`/`LowPriorityIO` pair.
/// Untested on this machine (task scope: stub behind `cfg(target_os =
/// "linux")` if untestable here) — written to the same shape as the exercised
/// macOS path, not exercised by any gate in this arc.
#[cfg(target_os = "linux")]
pub fn systemd_unit_contents(program: &Path) -> String {
    format!(
        r#"[Unit]
Description=dl (sprefa) reactive datalog-over-code daemon

[Service]
Type=notify
ExecStart={program} daemon serve
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=60
StartLimitBurst=5
Nice=10
IOSchedulingClass=idle
# CPUQuota=100% — left unset: needs a root-installed `Delegate=` drop-in on
# `user@.service` on most distros (plan section 3.2); the in-process
# CPU-percent governor (crate::budget::start_governor) is the enforced ceiling
# either way.

[Install]
WantedBy=default.target
"#,
        program = program.display(),
    )
}

#[cfg(target_os = "linux")]
fn systemd_unit_path() -> Result<PathBuf> {
    let home = home_dir()?;
    Ok(home
        .join(".config")
        .join("systemd")
        .join("user")
        .join(format!("{LABEL_APP}.service")))
}

/// Native, user-level service manager handle (`LaunchdServiceManager::user()`
/// on macOS, `SystemdServiceManager::user()` on Linux via
/// `service_manager::native()` + `set_level`). System/root-level service
/// managers are never selected — this is a user LaunchAgent/systemd user
/// unit, no elevated privileges.
fn native_user_manager() -> Result<Box<dyn service_manager::ServiceManager>> {
    let mut manager = service_manager::native_service_manager()
        .context("no native OS service manager available on this platform")?;
    manager
        .set_level(service_manager::ServiceLevel::User)
        .context("set service manager to user level")?;
    Ok(manager)
}

fn current_exe() -> Result<PathBuf> {
    std::env::current_exe().context("locate current dl executable")
}

/// `dl daemon install`: write the plist/unit (via `ServiceInstallCtx.contents`
/// — the crate's own templating is bypassed entirely) and register it with the
/// service manager. Never starts the service (matches the crate's own
/// cross-platform "install never auto-starts" convention) — `dl daemon start`
/// is the explicit trigger, so a fresh install leaves nothing running for
/// `dl daemon why` to have to explain.
pub fn install(home: &Path) -> Result<PathBuf> {
    let program = current_exe()?;
    let manager = native_user_manager()?;
    if !manager.available().unwrap_or(false) {
        bail!(
            "no OS service manager available (launchd/systemd) — supervision install requires one"
        );
    }
    let contents = plist_contents_for_platform(&program, home);
    let ctx = service_manager::ServiceInstallCtx {
        label: service_label(),
        program: program.clone(),
        args: vec!["daemon".into(), "serve".into()],
        contents: Some(contents),
        username: None,
        working_directory: None,
        environment: None,
        autostart: false,
        restart_policy: service_manager::RestartPolicy::default(),
    };
    manager
        .install(ctx)
        .context("install LaunchAgent/systemd unit")?;
    installed_unit_path()
}

#[cfg(target_os = "macos")]
fn plist_contents_for_platform(program: &Path, home: &Path) -> String {
    plist_contents(program, home)
}
#[cfg(target_os = "linux")]
fn plist_contents_for_platform(program: &Path, _home: &Path) -> String {
    systemd_unit_contents(program)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn plist_contents_for_platform(_program: &Path, _home: &Path) -> String {
    String::new()
}

#[cfg(target_os = "macos")]
fn installed_unit_path() -> Result<PathBuf> {
    plist_path()
}
#[cfg(target_os = "linux")]
fn installed_unit_path() -> Result<PathBuf> {
    systemd_unit_path()
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn installed_unit_path() -> Result<PathBuf> {
    bail!("supervision install is unsupported on this platform")
}

/// `dl daemon uninstall`: stop if running (best-effort — `uninstall` must not
/// leave an orphaned process even if the caller forgot `stop` first), then let
/// the service manager remove the plist/unit and deregister it. Leaves NO
/// trace in `launchctl print gui/$UID` (macOS) / `systemctl --user status`
/// (Linux) — the round-trip gate this arc verifies.
pub fn uninstall() -> Result<()> {
    let _ = stop();
    let manager = native_user_manager()?;
    let ctx = service_manager::ServiceUninstallCtx {
        label: service_label(),
    };
    manager
        .uninstall(ctx)
        .context("uninstall LaunchAgent/systemd unit")?;
    Ok(())
}

pub fn is_installed() -> bool {
    installed_unit_path().map(|p| p.exists()).unwrap_or(false)
}

/// `dl daemon start` under supervision: the plan's literal CLI mapping (section
/// 3.1) is `bootstrap` + `kickstart`. `install()` already registered the job
/// via the crate's own `launchctl load`, so `bootstrap` here is defensive (a
/// job `stop` (`bootout`) removed from the domain, or an install from a prior
/// process) — its "already bootstrapped" error is swallowed, not propagated.
/// `kickstart -p` is what actually fires it: `RunAtLoad`/`load` alone do not
/// start an on-demand job when the calling session's `gui/$UID` domain is in
/// on-demand-only mode (observed live during the gate-2 probe on this
/// machine), so `start` cannot rely on load-implies-run.
#[cfg(target_os = "macos")]
pub fn start() -> Result<()> {
    let plist = plist_path()?;
    if !plist.exists() {
        bail!(
            "supervision not installed ({} missing) — run `dl daemon install` first",
            plist.display()
        );
    }
    let _ = launchctl(&["bootstrap", &gui_domain(), &plist.to_string_lossy()]);
    run_launchctl_checked(&["kickstart", "-p", &domain_target()])?;
    Ok(())
}
#[cfg(target_os = "linux")]
pub fn start() -> Result<()> {
    if !systemd_unit_path()?.exists() {
        bail!("supervision not installed — run `dl daemon install` first");
    }
    run_systemctl_checked(&["--user", "start", &format!("{LABEL_APP}.service")])
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn start() -> Result<()> {
    bail!("supervision start is unsupported on this platform")
}

/// `dl daemon stop` under supervision: `bootout` per the plan's mapping —
/// unloads the job from the domain AND terminates the running process (within
/// `ExitTimeOut`). Not merely "stop the process": a job left loaded-but-
/// stopped would still show up in `launchctl print gui/$UID`, which the
/// install/uninstall round-trip's "nothing left running or installed" check
/// greps for. "Not currently bootstrapped" is a stopped-already outcome, not
/// an error — swallowed the same way the bespoke `stop()` treats "not
/// running" as success.
#[cfg(target_os = "macos")]
pub fn stop() -> Result<()> {
    let _ = launchctl(&["bootout", &domain_target()]);
    Ok(())
}
#[cfg(target_os = "linux")]
pub fn stop() -> Result<()> {
    let _ = Command::new("systemctl")
        .args(["--user", "stop", &format!("{LABEL_APP}.service")])
        .output();
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn stop() -> Result<()> {
    Ok(())
}

/// `dl daemon restart` under supervision: `kickstart -k` per the plan's
/// mapping — kills the running job and restarts it in one call; `-p` also
/// prints the new pid, which is the plan's documented stale-binary remedy
/// (`stale_binary::warn_if_stale` tells the human to redeploy; this is the
/// mechanical follow-through once they have). Falls back to a plain `start()`
/// when the job was never bootstrapped (kickstart on an unknown target
/// errors), so `restart` works as "start if down, else replace" either way.
#[cfg(target_os = "macos")]
pub fn restart() -> Result<()> {
    if run_launchctl_checked(&["kickstart", "-k", "-p", &domain_target()]).is_err() {
        start()?;
    }
    Ok(())
}
#[cfg(target_os = "linux")]
pub fn restart() -> Result<()> {
    run_systemctl_checked(&["--user", "restart", &format!("{LABEL_APP}.service")])
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn restart() -> Result<()> {
    bail!("supervision restart is unsupported on this platform")
}

/// Raw `launchctl print` text for `dl daemon why`/debug fallbacks — the plan
/// (section 3.1) is explicit that its format is not API-stable, so nothing
/// parses it structurally; liveness stays on the daemon's own heartbeat
/// (`crate::daemon::status`, the socket ping). This is second-witness text
/// only.
#[cfg(target_os = "macos")]
pub fn debug_print() -> Option<String> {
    launchctl(&["print", &domain_target()])
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}
#[cfg(not(target_os = "macos"))]
pub fn debug_print() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn launchctl(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("launchctl").args(args).output()
}

#[cfg(target_os = "macos")]
fn run_launchctl_checked(args: &[&str]) -> Result<()> {
    let out = launchctl(args).with_context(|| format!("spawn launchctl {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "launchctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn run_systemctl_checked(args: &[&str]) -> Result<()> {
    let out = Command::new("systemctl")
        .args(args)
        .output()
        .with_context(|| format!("spawn systemctl {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "systemctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_is_stable() {
        assert_eq!(qualified_label(), "com.sprefa.dl");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plist_contents_carries_the_gate2_decision() {
        let text = plist_contents(Path::new("/usr/local/bin/dl"), Path::new("/tmp/dl-home"));
        assert!(text.contains("<key>Nice</key>"));
        assert!(text.contains("<key>LowPriorityIO</key>"));
        // Gate 2 (plan section 5.2 q2, RESOLVED 2026-07-18): ProcessType=Background
        // measured ~45% of baseline CPU throughput; deliberately not set.
        assert!(!text.contains("ProcessType"));
        assert!(text.contains("<key>KeepAlive</key>"));
        assert!(text.contains("SuccessfulExit"));
        assert!(text.contains("daemon"));
        assert!(text.contains("serve"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn domain_target_is_gui_scoped() {
        assert!(domain_target().starts_with("gui/"));
        assert!(domain_target().ends_with(&qualified_label()));
    }
}
