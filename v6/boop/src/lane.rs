//! Lane names derived from one branch name: `feature/schema-emit` is the lane
//! id, the tmux session and the worktree path. `--lane <id>` still spawns.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::bus::Route;

/// The worktree parent directory, relative to the repo root.
pub const WORKTREE_ROOT: &str = ".boop-worktrees";

/// Conventional prefixes; nothing here rejects another one.
pub const KINDS: [&str; 4] = ["feature", "fix", "refactor", "chore"];

/// Model spelling -> harness, longest prefix first. A `provider/model` spelling
/// is opencode's; `gpt-5.6-luna@medium` is codex's.
const MODEL_HARNESS: [(&str, &str); 9] = [
    ("gpt", "codex"),
    ("codex", "codex"),
    ("o3", "codex"),
    ("o4", "codex"),
    ("claude", "claude"),
    ("opus", "claude"),
    ("sonnet", "claude"),
    ("haiku", "claude"),
    ("kimi", "kimi"),
];

/// Every name one spawn answers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaneIdentity {
    pub lane: String,
    pub tmux: String,
    pub branch: String,
    /// `None` runs the spawn in the repo itself (the pre-branch lane shape).
    pub worktree_dir: Option<PathBuf>,
}

/// A base commit and the rev that produced it, so a dry-run prints both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseSha {
    pub sha: String,
    pub rev: String,
}

/// A parent lane and the rung that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentPick {
    pub parent: Option<String>,
    pub source: &'static str,
}

/// The kind prefix of a branch, when it is one of `KINDS`.
pub fn kind_of(branch: &str) -> Option<&'static str> {
    let (prefix, rest) = branch.split_once('/')?;
    if rest.is_empty() {
        return None;
    }
    KINDS.into_iter().find(|kind| *kind == prefix)
}

/// A branch name as one shell word. tmux rejects `.` and `:` in a session name,
/// so every byte outside `[A-Za-z0-9_-]` collapses into a single `-`.
pub fn slug(branch: &str) -> String {
    let mut slugged = String::with_capacity(branch.len());
    for character in branch.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            slugged.push(character);
        } else if !slugged.ends_with('-') {
            slugged.push('-');
        }
    }
    slugged.trim_matches('-').to_owned()
}

/// The directory a branch's worktree lives in; slashes stay slashes, which is
/// the path the `lane/*` worktrees already on disk have.
pub fn worktree_dir(repo: &Path, branch: &str) -> PathBuf {
    let mut path = repo.join(WORKTREE_ROOT);
    for segment in branch.split('/').filter(|segment| !segment.is_empty()) {
        path.push(segment);
    }
    path
}

/// Resolve one spawn's names. A branch means a worktree; without one the caller
/// names the lane and the spawn runs in the repo.
pub fn derive(
    repo: &Path,
    branch: Option<&str>,
    lane_override: Option<&str>,
    tmux_override: Option<&str>,
) -> Result<LaneIdentity> {
    let Some(branch) = branch else {
        let Some(lane) = lane_override.filter(|lane| !lane.is_empty()) else {
            anyhow::bail!(
                "name the lane: --branch feature/<name> (also fix/, refactor/, chore/), or the older --lane <id>"
            );
        };
        return Ok(LaneIdentity {
            tmux: tmux_override.unwrap_or(lane).to_owned(),
            branch: tmux_override.unwrap_or(lane).to_owned(),
            lane: lane.to_owned(),
            worktree_dir: None,
        });
    };
    check_branch(branch)?;
    let lane = match lane_override.filter(|lane| !lane.is_empty()) {
        Some(lane) => lane.to_owned(),
        None => slug(branch),
    };
    if lane.is_empty() {
        anyhow::bail!("branch `{branch}` has no name left after slugging; pass --lane <id>");
    }
    Ok(LaneIdentity {
        tmux: tmux_override.unwrap_or(&lane).to_owned(),
        lane,
        branch: branch.to_owned(),
        worktree_dir: Some(worktree_dir(repo, branch)),
    })
}

/// A branch name that also has to be a path segment under `.boop-worktrees`.
fn check_branch(branch: &str) -> Result<()> {
    if branch.is_empty() {
        anyhow::bail!("--branch is empty");
    }
    if branch.starts_with('/') || branch.ends_with('/') {
        anyhow::bail!("--branch `{branch}` cannot start or end with `/`");
    }
    if branch.split('/').any(|segment| segment.is_empty()) {
        anyhow::bail!("--branch `{branch}` has an empty path segment");
    }
    if branch.contains("..") {
        anyhow::bail!("--branch `{branch}` cannot contain `..`");
    }
    if branch
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        anyhow::bail!("--branch `{branch}` cannot contain whitespace");
    }
    Ok(())
}

/// The repo a spawn defaults to. Inside a linked worktree that is the repo
/// owning it, so `.boop-worktrees` never nests inside a worktree.
pub fn repo_root(from: &Path) -> Result<PathBuf> {
    let toplevel = git_line(from, &["rev-parse", "--show-toplevel"])
        .with_context(|| format!("no git repo at {}; pass --cwd <repo>", from.display()))?;
    let common = git_line(
        from,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    if let Some(common) = common.map(PathBuf::from) {
        if common.file_name().and_then(|name| name.to_str()) == Some(".git") {
            if let Some(parent) = common.parent() {
                return Ok(parent.to_path_buf());
            }
        }
    }
    Ok(PathBuf::from(toplevel))
}

/// The commit a lane branches from when the caller pinned none. Local refs
/// only; a stale `origin/main` is the caller's fetch to do.
pub fn default_base_sha(repo: &Path) -> Result<BaseSha> {
    for rev in ["origin/main", "origin/HEAD", "HEAD"] {
        if let Some(sha) = rev_parse(repo, rev) {
            return Ok(BaseSha {
                sha,
                rev: rev.to_owned(),
            });
        }
    }
    anyhow::bail!(
        "no base commit in {}: origin/main, origin/HEAD and HEAD all fail to resolve; pass --base-sha",
        repo.display()
    )
}

/// The commit a rev names, or `None` when the rev resolves to no commit.
pub fn rev_parse(repo: &Path, rev: &str) -> Option<String> {
    git_line(
        repo,
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
}

fn git_line(repo: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!line.is_empty()).then_some(line)
}

/// The UTF-8 locale a spawn falls back to when the caller has none. macOS
/// ships no `C.UTF-8`; minimal Linux images ship no `en_US.UTF-8`.
pub const FALLBACK_LOCALE: &str = if cfg!(target_os = "macos") {
    "en_US.UTF-8"
} else {
    "C.UTF-8"
};

/// `LC_ALL` and `LANG` for a spawn, as a shell prefix. A tmux server started
/// outside a login shell hands its sessions a bare `C`, failing UTF-8 gates.
pub fn locale_stamp() -> String {
    let locale = shell_word(&utf8_locale());
    format!("LC_ALL={locale} LANG={locale}")
}

/// The caller's own locale when it is UTF-8, else `FALLBACK_LOCALE`.
pub fn utf8_locale() -> String {
    locale_from(
        std::env::var("LC_ALL").ok().as_deref(),
        std::env::var("LANG").ok().as_deref(),
    )
}

fn locale_from(lc_all: Option<&str>, lang: Option<&str>) -> String {
    [lc_all, lang]
        .into_iter()
        .flatten()
        .find(|value| is_utf8(value))
        .unwrap_or(FALLBACK_LOCALE)
        .to_owned()
}

fn is_utf8(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .replace('-', "")
        .ends_with("utf8")
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// The harness a model spelling names, or `None` when it names none.
pub fn harness_for_model(model: &str) -> Option<&'static str> {
    let name = model.split('@').next().unwrap_or(model).trim();
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        return Some("opencode");
    }
    let name = name.to_ascii_lowercase();
    MODEL_HARNESS
        .into_iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map(|(_, harness)| harness)
}

/// A model family whose own harness runs on a flat-rate plan; opencode would
/// pay metered credit for it, so spawn refuses with no override.
fn plan_harness_family(model: &str) -> Option<&'static str> {
    let lowered = model.to_ascii_lowercase();
    let name = lowered.split('@').next().unwrap_or(&lowered).trim();
    let bare = name.rsplit('/').next().unwrap_or(name);
    [
        ("gpt", "codex"),
        ("codex", "codex"),
        ("o3", "codex"),
        ("o4", "codex"),
        ("claude", "claude"),
        ("opus", "claude"),
        ("sonnet", "claude"),
        ("haiku", "claude"),
        ("fable", "claude"),
        ("gemini", "gemini"),
    ]
    .into_iter()
    .find(|(prefix, _)| bare.starts_with(prefix))
    .map(|(_, harness)| harness)
}

/// The harness a spawn runs on: `--harness` wins, else the model spelling.
/// Plan-family models are refused on opencode even with `--harness opencode`.
pub fn harness_for_spawn(explicit: Option<&str>, model: Option<&str>) -> Result<String> {
    let model_named = model.filter(|model| !model.is_empty());
    if let Some(harness) = explicit.filter(|harness| !harness.is_empty()) {
        if harness == "opencode" {
            if let Some(model) = model_named {
                if let Some(owner) = plan_harness_family(model) {
                    anyhow::bail!(
                        "model `{model}` is BANNED from opencode: its family runs on the `{owner}` harness's flat-rate plan, and opencode would pay metered API credit for it. Spell the bare model name (no provider path) so the `{owner}` harness picks it up."
                    );
                }
            }
        }
        return Ok(harness.to_owned());
    }
    let Some(model) = model_named else {
        return Ok("opencode".to_owned());
    };
    let Some(harness) = harness_for_model(model) else {
        anyhow::bail!(
            "model `{model}` names no harness (gpt-* codex, kimi-* kimi, claude-* claude, provider/model opencode); pass --harness <id>"
        );
    };
    if harness == "opencode" {
        if let Some(owner) = plan_harness_family(model) {
            anyhow::bail!(
                "model `{model}` is BANNED from opencode: its family runs on the `{owner}` harness's flat-rate plan, and opencode would pay metered API credit for it. Spell the bare model name (no provider path) so the `{owner}` harness picks it up."
            );
        }
    }
    if harness == "claude" {
        anyhow::bail!(
            "model `{model}` is a Claude model, and Claude workers run as the coordinator's own Agent-tool subagents, never as tmux lanes; pass --harness claude to spawn one anyway"
        );
    }
    Ok(harness.to_owned())
}

/// Who gets the completion hail: the flag, else the caller (a spawner is the
/// parent of what it spawns), else a lone registered coordinator.
pub fn resolve_parent(
    explicit: Option<&str>,
    caller_lane: Option<&str>,
    routes: &BTreeMap<String, Route>,
) -> ParentPick {
    if let Some(parent) = explicit.filter(|parent| !parent.is_empty()) {
        return ParentPick {
            parent: Some(parent.to_owned()),
            source: "flag",
        };
    }
    if let Some(lane) = caller_lane.filter(|lane| !lane.is_empty()) {
        return ParentPick {
            parent: Some(lane.to_owned()),
            source: "caller",
        };
    }
    let mut coordinators = routes.keys().filter(|lane| lane.contains("coordinator"));
    let (Some(only), None) = (coordinators.next(), coordinators.next()) else {
        return ParentPick {
            parent: None,
            source: "none",
        };
    };
    ParentPick {
        parent: Some(only.clone()),
        source: "registry",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::bus::Route;

    use super::{
        default_base_sha, derive, harness_for_model, harness_for_spawn, kind_of, repo_root,
        resolve_parent, rev_parse, slug, LaneIdentity, FALLBACK_LOCALE,
    };

    fn repo() -> PathBuf {
        PathBuf::from("/repo")
    }

    /// RECEIPT. Each conventional kind names a lane the same way, with no
    /// `lane/` prefix anywhere and one slug for both lane id and tmux.
    #[test]
    fn each_conventional_kind_derives_the_same_shape() {
        for kind in super::KINDS {
            let branch = format!("{kind}/schema-emit");
            let identity = derive(&repo(), Some(&branch), None, None).unwrap();
            assert_eq!(
                identity,
                LaneIdentity {
                    lane: format!("{kind}-schema-emit"),
                    tmux: format!("{kind}-schema-emit"),
                    branch: branch.clone(),
                    worktree_dir: Some(
                        PathBuf::from("/repo/.boop-worktrees")
                            .join(kind)
                            .join("schema-emit")
                    ),
                },
                "branch {branch}"
            );
            assert_eq!(kind_of(&branch), Some(kind));
        }
    }

    /// RECEIPT. A slash is spelled `-` in the lane id and tmux session, as is
    /// every character tmux rejects.
    #[test]
    fn slugging_spells_a_slash_as_a_dash() {
        assert_eq!(slug("feature/schema-emit"), "feature-schema-emit");
        assert_eq!(slug("fix/v6.2/parse"), "fix-v6-2-parse");
        assert_eq!(slug("chore/a:b"), "chore-a-b");
        assert_eq!(slug("refactor//double//slash"), "refactor-double-slash");
        assert_eq!(slug("/leading/and/trailing/"), "leading-and-trailing");
        assert_eq!(slug("keeps_underscores"), "keeps_underscores");
    }

    /// RECEIPT. The prefix is a convention, never a gate: an unconventional
    /// branch still spawns and reports no kind.
    #[test]
    fn an_unconventional_prefix_still_derives() {
        let identity = derive(&repo(), Some("lane/boop-sql"), None, None).unwrap();
        assert_eq!(identity.lane, "lane-boop-sql");
        assert_eq!(
            identity.worktree_dir,
            Some(PathBuf::from("/repo/.boop-worktrees/lane/boop-sql")),
            "the worktrees already on disk keep their path"
        );
        assert_eq!(kind_of("lane/boop-sql"), None);
        assert_eq!(kind_of("feature"), None, "a bare kind names nothing");
        assert_eq!(kind_of("feature/"), None);
    }

    /// RECEIPT. The pre-branch shape (`--lane <id>` alone) keeps its exact
    /// behavior: no worktree, branch equal to the lane id.
    #[test]
    fn the_older_lane_shape_still_derives_without_a_worktree() {
        let identity = derive(&repo(), None, Some("boop-sql"), None).unwrap();
        assert_eq!(
            identity,
            LaneIdentity {
                lane: "boop-sql".into(),
                tmux: "boop-sql".into(),
                branch: "boop-sql".into(),
                worktree_dir: None,
            }
        );
        let with_tmux = derive(&repo(), None, Some("boop-sql"), Some("pane-7")).unwrap();
        assert_eq!(with_tmux.tmux, "pane-7");
        assert_eq!(with_tmux.branch, "pane-7", "the pre-branch fallback order");
    }

    /// RECEIPT. A lane id already in the registry respawns under a new branch
    /// without renaming the row.
    #[test]
    fn overrides_win_over_the_derived_names() {
        let identity = derive(
            &repo(),
            Some("feature/schema-emit"),
            Some("schema-emit"),
            Some("old-pane"),
        )
        .unwrap();
        assert_eq!(identity.lane, "schema-emit");
        assert_eq!(identity.tmux, "old-pane");
        assert_eq!(identity.branch, "feature/schema-emit");
    }

    #[test]
    fn a_nameless_spawn_is_an_error_naming_the_flag() {
        let error = derive(&repo(), None, None, None).unwrap_err().to_string();
        assert!(error.contains("--branch feature/"), "message: {error}");
        assert!(error.contains("--lane"), "message: {error}");
    }

    /// RECEIPT. A branch that would escape `.boop-worktrees` is refused before
    /// git or the filesystem sees it.
    #[test]
    fn a_branch_that_escapes_the_worktree_root_is_refused() {
        for bad in ["../../etc", "/absolute", "trailing/", "has space", "a//b"] {
            assert!(
                derive(&repo(), Some(bad), None, None).is_err(),
                "branch {bad} must be refused"
            );
        }
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn seed_repo(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("boop-lane-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .arg(&path)
            .status()
            .unwrap();
        git(&path, &["config", "user.email", "t@t"]);
        git(&path, &["config", "user.name", "t"]);
        std::fs::write(path.join("seed.txt"), "seed").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-qm", "seed"]);
        path
    }

    /// RECEIPT. The default base is `origin/main`, read from local refs: the
    /// fixture writes the remote ref by hand, so no test touches the network.
    #[test]
    fn the_default_base_sha_prefers_origin_main() {
        let path = seed_repo("base");
        let head = rev_parse(&path, "HEAD").unwrap();
        assert_eq!(
            default_base_sha(&path).unwrap().rev,
            "HEAD",
            "with no remote ref the fallback is the local head"
        );
        std::fs::write(path.join("next.txt"), "next").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-qm", "next"]);
        let moved_head = rev_parse(&path, "HEAD").unwrap();
        git(&path, &["update-ref", "refs/remotes/origin/main", &head]);
        let base = default_base_sha(&path).unwrap();
        assert_eq!(base.rev, "origin/main");
        assert_eq!(base.sha, head, "origin/main wins over the local head");
        assert_ne!(base.sha, moved_head);
        let _ = std::fs::remove_dir_all(&path);
    }

    /// RECEIPT. Without origin/main, origin/HEAD still beats the local head.
    #[test]
    fn the_default_base_sha_falls_back_to_origin_head() {
        let path = seed_repo("originhead");
        let head = rev_parse(&path, "HEAD").unwrap();
        git(&path, &["update-ref", "refs/remotes/origin/trunk", &head]);
        git(
            &path,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        );
        std::fs::write(path.join("next.txt"), "next").unwrap();
        git(&path, &["add", "-A"]);
        git(&path, &["commit", "-qm", "next"]);
        let base = default_base_sha(&path).unwrap();
        assert_eq!(base.rev, "origin/HEAD");
        assert_eq!(base.sha, head);
        let _ = std::fs::remove_dir_all(&path);
    }

    /// RECEIPT. A spawn issued from inside a worktree branches the repo that
    /// owns it, so worktrees never nest.
    #[test]
    fn the_default_repo_from_a_worktree_is_the_owning_repo() {
        let path = seed_repo("root");
        let nested = path.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(
            std::fs::canonicalize(repo_root(&nested).unwrap()).unwrap(),
            std::fs::canonicalize(&path).unwrap()
        );
        let linked = path.join(".boop-worktrees/feature/x");
        git(
            &path,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature/x",
                &linked.display().to_string(),
            ],
        );
        assert_eq!(
            std::fs::canonicalize(repo_root(&linked).unwrap()).unwrap(),
            std::fs::canonicalize(&path).unwrap(),
            "a linked worktree resolves to the repo that owns it"
        );
        let _ = std::fs::remove_dir_all(&path);
    }

    fn route(tmux: &str) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some("opencode".into()),
            tmux: Some(tmux.into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
        }
    }

    /// RECEIPT. The parent ladder stops at the first rung that answers: flag,
    /// caller, lone coordinator.
    #[test]
    fn the_parent_ladder_stops_at_the_first_rung_that_answers() {
        let mut routes = BTreeMap::new();
        routes.insert("sprefa-coordinator".to_owned(), route("shell:0.0"));
        routes.insert("boop-sql".to_owned(), route("boop-sql"));

        let flagged = resolve_parent(Some("named"), Some("caller"), &routes);
        assert_eq!(flagged.parent.as_deref(), Some("named"));
        assert_eq!(flagged.source, "flag");

        let from_caller = resolve_parent(None, Some("caller"), &routes);
        assert_eq!(from_caller.parent.as_deref(), Some("caller"));
        assert_eq!(from_caller.source, "caller");

        let from_registry = resolve_parent(None, None, &routes);
        assert_eq!(from_registry.parent.as_deref(), Some("sprefa-coordinator"));
        assert_eq!(from_registry.source, "registry");
    }

    /// RECEIPT (field, 2026-08-10). `--model gpt-5.6-luna@medium` with no
    /// `--harness` dry-ran as opencode; the spelling names the harness now.
    #[test]
    fn a_gpt_model_names_the_codex_harness() {
        assert_eq!(harness_for_model("gpt-5.6-luna@medium"), Some("codex"));
        assert_eq!(
            harness_for_spawn(None, Some("gpt-5.6-luna@medium")).unwrap(),
            "codex"
        );
        assert_eq!(harness_for_model("kimi-k2"), Some("kimi"));
        assert_eq!(
            harness_for_model("openrouter/deepseek/deepseek-v4-flash-0731"),
            Some("opencode")
        );
        assert_eq!(
            harness_for_model("zai-coding-plan/glm-4.6"),
            Some("opencode")
        );
        assert_eq!(harness_for_model("nothing-known"), None);
    }

    /// RECEIPT (field, 2026-08-11). `openrouter/openai/gpt-5.6-sol` spawned
    /// two dead opencode lanes AND billed openrouter for plan-covered models.
    #[test]
    fn plan_family_models_are_banned_from_opencode() {
        let err = harness_for_spawn(None, Some("openrouter/openai/gpt-5.6-sol"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("BANNED from opencode"), "{err}");
        assert!(err.contains("codex"), "{err}");
        let err = harness_for_spawn(Some("opencode"), Some("openai/gpt-5.6-terra"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("BANNED from opencode"), "{err}");
        let err = harness_for_spawn(None, Some("anthropic/claude-sonnet-5"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("claude"), "{err}");
        let err = harness_for_spawn(Some("opencode"), Some("google/gemini-3-pro"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("gemini"), "{err}");
        assert_eq!(
            harness_for_spawn(None, Some("openrouter/deepseek/deepseek-v4-flash-0731")).unwrap(),
            "opencode"
        );
        assert_eq!(
            harness_for_spawn(None, Some("zai-coding-plan/glm-4.6")).unwrap(),
            "opencode"
        );
    }

    /// RECEIPT. An explicit --harness always wins, and no model at all keeps
    /// the opencode default the flash4 lanes run on.
    #[test]
    fn an_explicit_harness_wins_over_the_model_spelling() {
        assert_eq!(
            harness_for_spawn(Some("kimi"), Some("gpt-5.6-luna")).unwrap(),
            "kimi"
        );
        assert_eq!(harness_for_spawn(None, None).unwrap(), "opencode");
        assert_eq!(
            harness_for_spawn(Some("claude"), Some("claude-opus-4")).unwrap(),
            "claude",
            "the Agent-tool law is a default, and --harness claude is the way past it"
        );
    }

    /// RECEIPT. A Claude model without --harness stops, and an unknown
    /// spelling stops too rather than spawning on the wrong harness.
    #[test]
    fn an_unnamed_harness_never_guesses_opencode() {
        let claude = harness_for_spawn(None, Some("claude-opus-4"))
            .unwrap_err()
            .to_string();
        assert!(claude.contains("Agent-tool"), "message: {claude}");
        let unknown = harness_for_spawn(None, Some("nothing-known"))
            .unwrap_err()
            .to_string();
        assert!(unknown.contains("--harness"), "message: {unknown}");
        assert!(unknown.contains("nothing-known"), "message: {unknown}");
    }

    /// RECEIPT (field). Two luna lanes failed SWI-Prolog UTF-8 gates that pass
    /// in a shell: a lane inherits the tmux server's bare locale, not a shell's.
    #[test]
    fn a_spawn_always_gets_a_utf8_locale() {
        assert_eq!(
            super::locale_from(Some("en_US.UTF-8"), None),
            "en_US.UTF-8",
            "the caller's own locale is inherited when it is UTF-8"
        );
        assert_eq!(
            super::locale_from(Some("C"), Some("en_GB.UTF-8")),
            "en_GB.UTF-8",
            "LC_ALL is read first, but a non-UTF-8 one never wins"
        );
        assert_eq!(
            super::locale_from(Some("C"), Some("POSIX")),
            FALLBACK_LOCALE
        );
        assert_eq!(super::locale_from(None, None), FALLBACK_LOCALE);
        assert!(
            FALLBACK_LOCALE.to_ascii_lowercase().contains("utf"),
            "the fallback has to be a UTF-8 locale: {FALLBACK_LOCALE}"
        );
        assert!(super::is_utf8("ja_JP.utf8"), "the dashless spelling counts");
    }

    /// RECEIPT. The stamp sets both variables, quoted, so a locale with no
    /// shell-safe spelling cannot split the command.
    #[test]
    fn the_locale_stamp_sets_lc_all_and_lang() {
        let stamp = super::locale_stamp();
        assert!(stamp.starts_with("LC_ALL='"), "stamp: {stamp}");
        assert!(stamp.contains(" LANG='"), "stamp: {stamp}");
        assert_eq!(
            stamp.matches("UTF-8'").count() + stamp.matches("utf8'").count(),
            2,
            "both variables carry the same UTF-8 locale: {stamp}"
        );
    }

    /// RECEIPT. Two coordinators are ambiguous, so nothing is guessed.
    #[test]
    fn two_coordinators_default_to_no_parent() {
        let mut routes = BTreeMap::new();
        routes.insert("sprefa-coordinator".to_owned(), route("a"));
        routes.insert("instant-coordinator".to_owned(), route("b"));
        let pick = resolve_parent(None, None, &routes);
        assert_eq!(pick.parent, None);
        assert_eq!(pick.source, "none");
        assert_eq!(resolve_parent(None, None, &BTreeMap::new()).source, "none");
    }
}
