//! LOCAL LOCATE ONLY: an absent checkout becomes a `dep_unresolved` row, never
//! a network call. Acquisition is the separate remote-acquisition-policy call.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::types::{Arrival, ArrivalSign, RowColumnType, Value};

pub const DEP_REPO_COLUMNS: &[&str] = &["coordinate", "root"];
pub const DEP_EDGE_COLUMNS: &[&str] = &["from_repo", "from_rev", "target", "to_repo"];
pub const DEP_UNRESOLVED_COLUMNS: &[&str] = &["from_repo", "target", "reason"];
pub const DEP_VISITED_COLUMNS: &[&str] = &["repo", "rev", "hop"];

/// Every column is a scalar, so a consuming program reads a frontier without
/// the struct plane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepResolveRelations {
    pub repo: String,
    pub edge: String,
    pub unresolved: String,
    pub visited: String,
}

impl Default for DepResolveRelations {
    fn default() -> Self {
        Self {
            repo: "dep_repo".to_string(),
            edge: "dep_edge".to_string(),
            unresolved: "dep_unresolved".to_string(),
            visited: "dep_visited".to_string(),
        }
    }
}

impl DepResolveRelations {
    pub fn declarations(&self) -> [DepResolveRelation; 4] {
        [
            DepResolveRelation {
                name: self.repo.clone(),
                columns: DEP_REPO_COLUMNS,
                column_types: &[RowColumnType::Text, RowColumnType::Text],
            },
            DepResolveRelation {
                name: self.edge.clone(),
                columns: DEP_EDGE_COLUMNS,
                column_types: &[
                    RowColumnType::Text,
                    RowColumnType::Text,
                    RowColumnType::Text,
                    RowColumnType::Text,
                ],
            },
            DepResolveRelation {
                name: self.unresolved.clone(),
                columns: DEP_UNRESOLVED_COLUMNS,
                column_types: &[RowColumnType::Text, RowColumnType::Text, RowColumnType::Text],
            },
            DepResolveRelation {
                name: self.visited.clone(),
                columns: DEP_VISITED_COLUMNS,
                column_types: &[RowColumnType::Text, RowColumnType::Text, RowColumnType::Int],
            },
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepResolveRelation {
    pub name: String,
    pub columns: &'static [&'static str],
    pub column_types: &'static [RowColumnType],
}

/// The coordinate a dependency target names a repository by, and the checkout
/// that answers for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalRepo {
    pub coordinate: String,
    pub root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisitedRepo {
    pub coordinate: String,
    pub revision: String,
    pub hop: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepEdge {
    pub from_repo: String,
    pub from_revision: String,
    pub target: String,
    pub to_repo: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedTarget {
    pub from_repo: String,
    pub target: String,
    pub reason: UnresolvedReason,
}

/// Each spelling is a row value, so a consuming rule filters on it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UnresolvedReason {
    RelativePath,
    NoLocalCheckout,
    RevisionUnavailable,
}

impl UnresolvedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelativePath => "relative_path",
            Self::NoLocalCheckout => "no_local_checkout",
            Self::RevisionUnavailable => "revision_unavailable",
        }
    }
}

/// `VisitBudget` is a backstop the visited set makes unreachable; it reports
/// instead of running forever if a future frontier breaks that.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    FrontierClosed,
    VisitBudget,
}

/// Construction reads each checkout's manifests at Git HEAD; resolution never
/// touches the filesystem.
#[derive(Clone, Debug, Default)]
pub struct LocalRepoRoster {
    entries: BTreeMap<String, PathBuf>,
}

impl LocalRepoRoster {
    pub fn from_entries(entries: impl IntoIterator<Item = (String, PathBuf)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /// One directory may answer to several coordinates: its `go.mod` module
    /// path, its `package.json` name, and always its directory name.
    pub fn scan_checkout_root(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root = root.as_ref();
        let mut entries = BTreeMap::new();
        let mut checkouts = CheckoutTrees::default();
        let listing = std::fs::read_dir(root)
            .with_context(|| format!("scan checkout root {}", root.display()))?;
        for child in listing {
            let child = child?.path();
            if !child.is_dir() {
                continue;
            }
            let Some(name) = child.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            entries.insert(name.to_string(), child.clone());
            let repo = LocalRepo {
                coordinate: name.to_string(),
                root: child.clone(),
            };
            let Ok(head) = checkouts.head(&repo) else {
                continue;
            };
            let _ = checkouts.read_each(
                &repo,
                &head,
                &["go.mod".to_string(), "package.json".to_string()],
                |path, bytes| match path {
                    "go.mod" => {
                        if let Some(module) = go_module_path(bytes) {
                            entries.insert(module, child.clone());
                        }
                    }
                    "package.json" => {
                        if let Some(package) = package_json_name(bytes) {
                            entries.insert(package, child.clone());
                        }
                    }
                    _ => {}
                },
            );
        }
        Ok(Self { entries })
    }

    pub fn insert(&mut self, coordinate: impl Into<String>, root: impl Into<PathBuf>) {
        self.entries.insert(coordinate.into(), root.into());
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A package path is deeper than its repository, so the match is the
    /// longest `/`-prefix that is a checkout.
    pub fn locate(&self, target: &str) -> Option<LocalRepo> {
        let mut prefix = target;
        loop {
            if let Some(root) = self.entries.get(prefix) {
                return Some(LocalRepo {
                    coordinate: prefix.to_string(),
                    root: root.clone(),
                });
            }
            match prefix.rsplit_once('/') {
                Some((shorter, _)) if !shorter.is_empty() => prefix = shorter,
                _ => return None,
            }
        }
    }
}

/// `revision_of` is asked once per coordinate, so a source that answers
/// differently on a later call cannot reopen a closed repository.
pub trait IDepFrontierSource {
    fn revision_of(&mut self, repo: &LocalRepo) -> anyhow::Result<String>;
    fn targets_at(&mut self, repo: &LocalRepo, revision: &str) -> anyhow::Result<Vec<String>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepResolveOutcome {
    pub visited: Vec<VisitedRepo>,
    pub repos: Vec<LocalRepo>,
    pub edges: Vec<DepEdge>,
    pub unresolved: Vec<UnresolvedTarget>,
    pub stopped: StopReason,
    relations: DepResolveRelations,
}

impl DepResolveOutcome {
    pub fn arrivals(&self) -> Vec<Arrival> {
        let add = |rel: &String, row: Vec<Value>| Arrival {
            rel: rel.clone(),
            sign: ArrivalSign::Add,
            row,
        };
        let mut arrivals = Vec::new();
        for repo in &self.repos {
            arrivals.push(add(
                &self.relations.repo,
                vec![
                    Value::Text(repo.coordinate.clone()),
                    Value::Text(repo.root.to_string_lossy().to_string()),
                ],
            ));
        }
        for at in &self.visited {
            arrivals.push(add(
                &self.relations.visited,
                vec![
                    Value::Text(at.coordinate.clone()),
                    Value::Text(at.revision.clone()),
                    Value::Integer(i64::from(at.hop)),
                ],
            ));
        }
        for edge in &self.edges {
            arrivals.push(add(
                &self.relations.edge,
                vec![
                    Value::Text(edge.from_repo.clone()),
                    Value::Text(edge.from_revision.clone()),
                    Value::Text(edge.target.clone()),
                    Value::Text(edge.to_repo.clone()),
                ],
            ));
        }
        for row in &self.unresolved {
            arrivals.push(add(
                &self.relations.unresolved,
                vec![
                    Value::Text(row.from_repo.clone()),
                    Value::Text(row.target.clone()),
                    Value::Text(row.reason.as_str().to_string()),
                ],
            ));
        }
        arrivals
    }
}

pub struct DepResolver<'a> {
    roster: &'a LocalRepoRoster,
    relations: DepResolveRelations,
}

impl<'a> DepResolver<'a> {
    pub fn new(roster: &'a LocalRepoRoster) -> Self {
        Self {
            roster,
            relations: DepResolveRelations::default(),
        }
    }

    pub fn with_relations(mut self, relations: DepResolveRelations) -> Self {
        self.relations = relations;
        self
    }

    pub fn relations(&self) -> &DepResolveRelations {
        &self.relations
    }

    pub fn locate(&self, target: &str) -> Option<LocalRepo> {
        self.roster.locate(target)
    }

    /// Every visited-set insertion consumes a distinct roster entry, so the
    /// pop count cannot exceed roster + seed size.
    pub fn run(
        &self,
        seeds: &[String],
        frontier: &mut dyn IDepFrontierSource,
    ) -> anyhow::Result<DepResolveOutcome> {
        let budget = self.roster.len() + seeds.len();
        let mut revisions: BTreeMap<String, String> = BTreeMap::new();
        let mut visited: BTreeSet<(String, String)> = BTreeSet::new();
        let mut queue: VecDeque<(LocalRepo, String, u32)> = VecDeque::new();
        let mut outcome = DepResolveOutcome {
            visited: Vec::new(),
            repos: Vec::new(),
            edges: Vec::new(),
            unresolved: Vec::new(),
            stopped: StopReason::FrontierClosed,
            relations: self.relations.clone(),
        };
        let mut announced: BTreeSet<String> = BTreeSet::new();

        for seed in seeds {
            match self.admit(seed, frontier, &mut revisions)? {
                Ok((repo, revision)) => {
                    if visited.insert((repo.coordinate.clone(), revision.clone())) {
                        announce(&mut outcome, &mut announced, &repo);
                        queue.push_back((repo, revision, 0));
                    }
                }
                Err(reason) => outcome.unresolved.push(UnresolvedTarget {
                    from_repo: seed.clone(),
                    target: seed.clone(),
                    reason,
                }),
            }
        }

        let mut pops = 0usize;
        while let Some((repo, revision, hop)) = queue.pop_front() {
            pops += 1;
            if pops > budget {
                outcome.stopped = StopReason::VisitBudget;
                break;
            }
            outcome.visited.push(VisitedRepo {
                coordinate: repo.coordinate.clone(),
                revision: revision.clone(),
                hop,
            });
            let targets: BTreeSet<String> =
                frontier.targets_at(&repo, &revision)?.into_iter().collect();
            for target in targets {
                match self.admit(&target, frontier, &mut revisions)? {
                    Ok((next, next_revision)) => {
                        outcome.edges.push(DepEdge {
                            from_repo: repo.coordinate.clone(),
                            from_revision: revision.clone(),
                            target: target.clone(),
                            to_repo: next.coordinate.clone(),
                        });
                        if visited.insert((next.coordinate.clone(), next_revision.clone())) {
                            announce(&mut outcome, &mut announced, &next);
                            queue.push_back((next, next_revision, hop + 1));
                        }
                    }
                    Err(reason) => outcome.unresolved.push(UnresolvedTarget {
                        from_repo: repo.coordinate.clone(),
                        target: target.clone(),
                        reason,
                    }),
                }
            }
        }
        Ok(outcome)
    }

    /// The revision cache is what bounds the crawl.
    fn admit(
        &self,
        target: &str,
        frontier: &mut dyn IDepFrontierSource,
        revisions: &mut BTreeMap<String, String>,
    ) -> anyhow::Result<Result<(LocalRepo, String), UnresolvedReason>> {
        if is_relative(target) {
            return Ok(Err(UnresolvedReason::RelativePath));
        }
        let Some(repo) = self.roster.locate(target) else {
            return Ok(Err(UnresolvedReason::NoLocalCheckout));
        };
        if let Some(revision) = revisions.get(&repo.coordinate) {
            return Ok(Ok((repo.clone(), revision.clone())));
        }
        match frontier.revision_of(&repo) {
            Ok(revision) => {
                revisions.insert(repo.coordinate.clone(), revision.clone());
                Ok(Ok((repo, revision)))
            }
            Err(_) => Ok(Err(UnresolvedReason::RevisionUnavailable)),
        }
    }
}

fn announce(outcome: &mut DepResolveOutcome, announced: &mut BTreeSet<String>, repo: &LocalRepo) {
    if announced.insert(repo.coordinate.clone()) {
        outcome.repos.push(repo.clone());
    }
}

fn is_relative(target: &str) -> bool {
    target.starts_with('.') || target.starts_with('/')
}

fn go_module_path(manifest: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(manifest).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("module "))
        .map(|path| path.trim().trim_matches('"').to_string())
        .filter(|path| !path.is_empty())
}

fn package_json_name(manifest: &[u8]) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_slice(manifest).ok()?;
    parsed
        .get("name")?
        .as_str()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Checkouts opened once per crawl. Soopy owns every Git call.
#[derive(Default)]
struct CheckoutTrees {
    trees: BTreeMap<String, soopy::SourceTree>,
}

impl CheckoutTrees {
    fn tree(&mut self, repo: &LocalRepo) -> anyhow::Result<&mut soopy::SourceTree> {
        if !self.trees.contains_key(&repo.coordinate) {
            let soopy::SourceRoot::GitWorktree(git) = soopy::SourceRoot::discover_git(&repo.root)?
            else {
                unreachable!("explicit Git discovery always returns a Git worktree")
            };
            self.trees.insert(
                repo.coordinate.clone(),
                soopy::SourceTree::open(git.repository.clone()),
            );
        }
        Ok(self
            .trees
            .get_mut(&repo.coordinate)
            .expect("tree was just inserted"))
    }

    fn head(&mut self, repo: &LocalRepo) -> anyhow::Result<String> {
        let revision = self
            .tree(repo)?
            .resolve_revision(soopy::Revision::Named("HEAD".into()))?;
        match revision {
            soopy::RevisionId::Commit(oid) => Ok(oid.0.to_string()),
            soopy::RevisionId::Worktree { .. } => anyhow::bail!(
                "HEAD resolved to a worktree coordinate in {}",
                repo.coordinate
            ),
        }
    }

    /// Bytes are visited through one reusable buffer and dropped, never
    /// accumulated.
    fn read_each<F>(
        &mut self,
        repo: &LocalRepo,
        revision: &str,
        pathspecs: &[String],
        mut visit: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(&str, &[u8]),
    {
        let selection = soopy::Revision::Commit(soopy::ObjectId(revision.into()));
        let tree = self.tree(repo)?;
        let entries = tree.git_files(&soopy::GitFilesQuery {
            revision: selection,
            pathspecs: pathspecs.to_vec(),
        })?;
        let requests: Vec<soopy::ReadRequest> = entries
            .into_iter()
            .map(|entry| soopy::ReadRequest {
                source: entry.source,
                expected: Some(entry.content),
            })
            .collect();
        let mut buffer = Vec::new();
        tree.read_each(&requests, &mut buffer, |answer| {
            visit(answer.source.path.0.as_ref(), answer.bytes);
            Ok(())
        })
    }
}

/// The same `FlatFact::Specifier` facts SourceBind projects into
/// `source_specifier`, read off the checkout at its pinned revision.
pub struct SpecifierFrontier {
    checkouts: CheckoutTrees,
    pathspecs: Vec<String>,
}

impl Default for SpecifierFrontier {
    fn default() -> Self {
        Self::new(vec![
            "*.ts".to_string(),
            "*.tsx".to_string(),
            "*.pl".to_string(),
            "*.dl6".to_string(),
        ])
    }
}

impl SpecifierFrontier {
    pub fn new(pathspecs: Vec<String>) -> Self {
        Self {
            checkouts: CheckoutTrees::default(),
            pathspecs,
        }
    }
}

impl IDepFrontierSource for SpecifierFrontier {
    fn revision_of(&mut self, repo: &LocalRepo) -> anyhow::Result<String> {
        self.checkouts.head(repo)
    }

    fn targets_at(&mut self, repo: &LocalRepo, revision: &str) -> anyhow::Result<Vec<String>> {
        let mut targets = Vec::new();
        let pathspecs = self.pathspecs.clone();
        self.checkouts
            .read_each(repo, revision, &pathspecs, |path, bytes| {
                let Some(output) =
                    sprefa_extract::dispatch(path, bytes, sprefa_extract::FamilyMask::ALL)
                else {
                    return;
                };
                for fact in sprefa_extract::flatten(&output) {
                    if let sprefa_extract::FlatFact::Specifier { name, module, .. } = fact {
                        targets.push(module.unwrap_or(name));
                    }
                }
            })?;
        Ok(targets)
    }
}

/// `sprefa-extract/src/lang/go.rs` has no Specifier arm, so a Go corpus reaches
/// its targets through `go.mod` requires instead.
pub struct GoModFrontier {
    checkouts: CheckoutTrees,
    require: regex::Regex,
}

impl Default for GoModFrontier {
    fn default() -> Self {
        Self::new()
    }
}

impl GoModFrontier {
    /// The pattern `goldens/multirepo_crawl/0_multirepo_crawl.dl6:75` selects
    /// pins with; the indent is optional because `1_corpus.sh:47` has none.
    pub fn new() -> Self {
        Self {
            checkouts: CheckoutTrees::default(),
            require: regex::Regex::new(
                r"(?m)^[ \t]*([a-zA-Z0-9._-]+/[a-zA-Z0-9._/-]+)[ \t]+(v[0-9][a-zA-Z0-9._+-]*)",
            )
            .expect("the require pattern is a literal"),
        }
    }
}

impl IDepFrontierSource for GoModFrontier {
    fn revision_of(&mut self, repo: &LocalRepo) -> anyhow::Result<String> {
        self.checkouts.head(repo)
    }

    fn targets_at(&mut self, repo: &LocalRepo, revision: &str) -> anyhow::Result<Vec<String>> {
        let mut targets = Vec::new();
        let pathspecs = vec!["go.mod".to_string()];
        let require = self.require.clone();
        self.checkouts
            .read_each(repo, revision, &pathspecs, |_path, bytes| {
                let Ok(text) = std::str::from_utf8(bytes) else {
                    return;
                };
                for capture in require.captures_iter(text) {
                    targets.push(capture[1].to_string());
                }
            })?;
        Ok(targets)
    }
}
