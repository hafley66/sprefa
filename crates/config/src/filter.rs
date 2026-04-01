use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::{FilterConfig, FilterMode, RepoConfig};

/// Compiled filter ready to test file paths against.
#[derive(Debug)]
pub struct CompiledFilter {
    mode: FilterMode,
    globs: GlobSet,
}

impl CompiledFilter {
    pub fn compile(config: &FilterConfig) -> anyhow::Result<Self> {
        let patterns = match config.mode {
            FilterMode::Exclude => config.exclude.as_deref().unwrap_or(&[]),
            FilterMode::Include => config.include.as_deref().unwrap_or(&[]),
        };

        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            builder.add(Glob::new(pattern)?);
        }

        Ok(CompiledFilter {
            mode: config.mode.clone(),
            globs: builder.build()?,
        })
    }

    /// Returns true if the path should be included (not filtered out).
    pub fn allows(&self, path: &str) -> bool {
        let matched = self.globs.is_match(path);
        match self.mode {
            FilterMode::Exclude => !matched,
            FilterMode::Include => matched,
        }
    }
}

/// Resolves the effective filter for a given repo + branch combination.
/// Priority: branch override > repo filter > global filter.
pub fn resolve_filter(
    global: Option<&FilterConfig>,
    repo: &RepoConfig,
    branch: &str,
) -> Option<FilterConfig> {
    // Check branch overrides first (most specific)
    if let Some(overrides) = &repo.branch_overrides {
        for ovr in overrides {
            if branch_matches(&ovr.branch, branch) {
                if ovr.filter.is_some() {
                    return ovr.filter.clone();
                }
            }
        }
    }

    // Then repo-level filter
    if repo.filter.is_some() {
        return repo.filter.clone();
    }

    // Fall back to global
    global.cloned()
}

fn branch_matches(pattern: &str, branch: &str) -> bool {
    if let Ok(glob) = Glob::new(pattern) {
        glob.compile_matcher().is_match(branch)
    } else {
        pattern == branch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BranchOverride;
    use insta::assert_yaml_snapshot;

    #[test]
    fn exclude_mode_filters_matched_paths() {
        let filter = CompiledFilter::compile(&FilterConfig {
            mode: FilterMode::Exclude,
            exclude: Some(vec![
                "node_modules/**".into(),
                "*.min.js".into(),
                "dist/**".into(),
            ]),
            include: None,
        })
        .unwrap();

        let paths = vec![
            "src/index.ts",
            "node_modules/foo/index.js",
            "dist/bundle.js",
            "lib/utils.ts",
            "app.min.js",
            "src/components/Button.tsx",
        ];

        let results: Vec<(&str, bool)> = paths.iter().map(|p| (*p, filter.allows(p))).collect();
        assert_yaml_snapshot!(results);
    }

    #[test]
    fn include_mode_only_allows_matched_paths() {
        let filter = CompiledFilter::compile(&FilterConfig {
            mode: FilterMode::Include,
            include: Some(vec!["src/**".into(), "lib/**".into()]),
            exclude: None,
        })
        .unwrap();

        let paths = vec![
            "src/index.ts",
            "node_modules/foo/index.js",
            "lib/utils.ts",
            "README.md",
        ];

        let results: Vec<(&str, bool)> = paths.iter().map(|p| (*p, filter.allows(p))).collect();
        assert_yaml_snapshot!(results);
    }

    #[test]
    fn resolve_filter_priority() {
        let global = FilterConfig {
            mode: FilterMode::Exclude,
            exclude: Some(vec!["node_modules/**".into()]),
            include: None,
        };

        let repo = RepoConfig {
            name: "test".into(),
            path: "/tmp/test".into(),
            revs: Some(vec!["main".into(), "release/v3".into()]),
            filter: Some(FilterConfig {
                mode: FilterMode::Exclude,
                exclude: Some(vec!["vendor/**".into()]),
                include: None,
            }),
            branch_overrides: Some(vec![BranchOverride {
                branch: "release/*".into(),
                filter: Some(FilterConfig {
                    mode: FilterMode::Include,
                    include: Some(vec!["src/**".into()]),
                    exclude: None,
                }),
                scan: None,
            }]),
            exclude_revs: None,
        };

        // branch override wins for release/v3
        let release_filter = resolve_filter(Some(&global), &repo, "release/v3");
        // repo filter wins for main (no branch override matches)
        let main_filter = resolve_filter(Some(&global), &repo, "main");

        assert_yaml_snapshot!("release_branch", release_filter);
        assert_yaml_snapshot!("main_branch", main_filter);
    }

    #[test]
    fn resolve_filter_falls_through_to_global() {
        let global = FilterConfig {
            mode: FilterMode::Exclude,
            exclude: Some(vec!["node_modules/**".into()]),
            include: None,
        };

        let repo = RepoConfig {
            name: "bare".into(),
            path: "/tmp/bare".into(),
            revs: None,
            filter: None,
            branch_overrides: None,
            exclude_revs: None,
        };

        let result = resolve_filter(Some(&global), &repo, "main");
        assert_yaml_snapshot!(result);
    }
}
