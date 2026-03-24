use std::path::{Path, PathBuf};

use crate::SourceConfig;

/// A repo discovered from a source root by matching its layout pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredRepo {
    pub org: Option<String>,
    pub name: String,
    pub branch: Option<String>,
    pub path: PathBuf,
}

/// Parse a layout pattern into ordered segments.
/// "{org}/{branch}/{repo}" -> [Org, Branch, Repo]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Org,
    Branch,
    Repo,
}

fn parse_layout(layout: &str) -> Vec<Segment> {
    layout
        .split('/')
        .filter(|s| !s.is_empty())
        .filter_map(|s| match s {
            "{org}" => Some(Segment::Org),
            "{branch}" => Some(Segment::Branch),
            "{repo}" => Some(Segment::Repo),
            _ => None,
        })
        .collect()
}

/// Walk a source root and discover repos matching the layout pattern.
///
/// The layout pattern defines the directory depth and what each level means.
/// For a 3-segment layout like `{org}/{branch}/{repo}`, sprefa expects
/// exactly 3 levels of directories under root, and maps each level to
/// its placeholder.
pub fn discover_repos(source: &SourceConfig) -> anyhow::Result<Vec<DiscoveredRepo>> {
    let segments = parse_layout(&source.layout);
    if segments.is_empty() {
        anyhow::bail!("layout pattern has no recognized placeholders: {}", source.layout);
    }
    if !segments.contains(&Segment::Repo) {
        anyhow::bail!("layout pattern must contain {{repo}}: {}", source.layout);
    }

    let root = crate::expand_tilde(&source.root);
    let root = Path::new(&root);

    if !root.is_dir() {
        return Ok(vec![]);
    }

    let mut results = Vec::new();
    walk_layout(root, &segments, 0, &mut LayoutState::default(), source, &mut results)?;
    Ok(results)
}

#[derive(Default, Clone)]
struct LayoutState {
    org: Option<String>,
    branch: Option<String>,
    repo: Option<String>,
}

fn walk_layout(
    dir: &Path,
    segments: &[Segment],
    depth: usize,
    state: &LayoutState,
    source: &SourceConfig,
    results: &mut Vec<DiscoveredRepo>,
) -> anyhow::Result<()> {
    if depth == segments.len() {
        // We've matched all segments, this directory is a checkout
        let repo_name = state
            .repo
            .clone()
            .expect("repo segment must be present (validated above)");

        let org = state.org.clone().or_else(|| source.default_org.clone());
        let branch = state.branch.clone().or_else(|| source.default_branch.clone());

        results.push(DiscoveredRepo {
            org,
            name: repo_name,
            branch,
            path: dir.to_owned(),
        });
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // skip unreadable dirs
    };

    for entry in entries {
        let entry = entry?;
        let ft = entry.file_type()?;
        if !ft.is_dir() && !ft.is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();

        // Skip hidden directories
        if name_str.starts_with('.') {
            continue;
        }

        let mut next_state = state.clone();
        match &segments[depth] {
            Segment::Org => next_state.org = Some(name_str),
            Segment::Branch => next_state.branch = Some(name_str),
            Segment::Repo => next_state.repo = Some(name_str),
        }

        walk_layout(&entry.path(), segments, depth + 1, &next_state, source, results)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_yaml_snapshot;
    use std::fs;

    fn setup_tree(root: &Path, dirs: &[&str]) {
        for d in dirs {
            fs::create_dir_all(root.join(d)).unwrap();
        }
    }

    #[test]
    fn discover_org_branch_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        setup_tree(root, &[
            "acme/main/frontend/src",
            "acme/main/backend/src",
            "acme/release-v3/frontend/src",
            "other-org/main/shared-lib/src",
        ]);

        let source = SourceConfig {
            root: root.to_string_lossy().to_string(),
            layout: "{org}/{branch}/{repo}".to_string(),
            default_org: None,
            default_branch: None,
            filter: None,
        };

        let mut repos = discover_repos(&source).unwrap();
        repos.sort_by(|a, b| (&a.org, &a.branch, &a.name).cmp(&(&b.org, &b.branch, &b.name)));

        // Snapshot just the logical fields, not the absolute paths
        let snapshot: Vec<_> = repos.iter().map(|r| {
            (r.org.as_deref(), r.branch.as_deref(), r.name.as_str())
        }).collect();
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn discover_org_repo_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        setup_tree(root, &[
            "acme/frontend/main",
            "acme/frontend/develop",
            "acme/backend/main",
        ]);

        let source = SourceConfig {
            root: root.to_string_lossy().to_string(),
            layout: "{org}/{repo}/{branch}".to_string(),
            default_org: None,
            default_branch: None,
            filter: None,
        };

        let mut repos = discover_repos(&source).unwrap();
        repos.sort_by(|a, b| (&a.org, &a.name, &a.branch).cmp(&(&b.org, &b.name, &b.branch)));

        let snapshot: Vec<_> = repos.iter().map(|r| {
            (r.org.as_deref(), r.name.as_str(), r.branch.as_deref())
        }).collect();
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn discover_flat_repo_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        setup_tree(root, &["frontend", "backend"]);

        let source = SourceConfig {
            root: root.to_string_lossy().to_string(),
            layout: "{repo}".to_string(),
            default_org: Some("myco".to_string()),
            default_branch: Some("main".to_string()),
            filter: None,
        };

        let mut repos = discover_repos(&source).unwrap();
        repos.sort_by(|a, b| a.name.cmp(&b.name));

        let snapshot: Vec<_> = repos.iter().map(|r| {
            (r.org.as_deref(), r.name.as_str(), r.branch.as_deref())
        }).collect();
        assert_yaml_snapshot!(snapshot);
    }

    #[test]
    fn discover_skips_hidden_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        setup_tree(root, &["acme/main/frontend", "acme/main/.git-cache"]);

        let source = SourceConfig {
            root: root.to_string_lossy().to_string(),
            layout: "{org}/{branch}/{repo}".to_string(),
            default_org: None,
            default_branch: None,
            filter: None,
        };

        let repos = discover_repos(&source).unwrap();
        let names: Vec<_> = repos.iter().map(|r| r.name.as_str()).collect();
        assert_yaml_snapshot!(names);
    }

    #[test]
    fn discover_empty_root_returns_empty() {
        let source = SourceConfig {
            root: "/nonexistent/path/that/does/not/exist".to_string(),
            layout: "{org}/{repo}".to_string(),
            default_org: None,
            default_branch: None,
            filter: None,
        };

        let repos = discover_repos(&source).unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn layout_requires_repo_placeholder() {
        let source = SourceConfig {
            root: "/tmp".to_string(),
            layout: "{org}/{branch}".to_string(),
            default_org: None,
            default_branch: None,
            filter: None,
        };

        let err = discover_repos(&source).unwrap_err();
        assert!(err.to_string().contains("{repo}"));
    }
}
