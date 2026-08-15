use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitSourceRegistration {
    pub repository: soopy::RepositoryId,
    pub worktree: soopy::WorktreeId,
}

/// Open filesystem roots owned by one engine runtime. Directory roots have no
/// Git coordinate. Git roots retain distinct checkout identity while linked
/// worktrees share their repository identity.
#[derive(Default)]
pub struct SourceInputs {
    directories: BTreeMap<soopy::DirectoryId, soopy::DirectoryRoot>,
    worktrees: BTreeMap<soopy::WorktreeId, soopy::GitWorktreeRoot>,
    repositories: BTreeMap<soopy::RepositoryId, Vec<soopy::WorktreeId>>,
}

impl SourceInputs {
    pub fn register_directory(&mut self, root: impl AsRef<Path>) -> Result<soopy::DirectoryId> {
        let directory = soopy::DirectoryRoot::open(root)?;
        let id = directory.identity.clone();
        self.directories.insert(id.clone(), directory);
        Ok(id)
    }

    pub fn register_git(&mut self, root: impl AsRef<Path>) -> Result<GitSourceRegistration> {
        let soopy::SourceRoot::GitWorktree(git) = soopy::SourceRoot::discover_git(root)? else {
            unreachable!("explicit Git discovery always returns a Git worktree")
        };
        let repository = git.repository.identity.clone();
        let worktree = git.repository.worktree.clone();
        let siblings = self.repositories.entry(repository.clone()).or_default();
        if !siblings.contains(&worktree) {
            siblings.push(worktree.clone());
        }
        self.worktrees.insert(worktree.clone(), *git);
        Ok(GitSourceRegistration {
            repository,
            worktree,
        })
    }

    pub fn directory_snapshot(
        &mut self,
        directory: &soopy::DirectoryId,
        query: &soopy::FileQuery,
    ) -> Result<soopy::FileSnapshot> {
        self.directories
            .get_mut(directory)
            .with_context(|| format!("directory root {} is not registered", directory.0))?
            .snapshot(query)
    }

    pub fn tracked_state(
        &mut self,
        worktree: &soopy::WorktreeId,
        query: &soopy::GitFileQuery,
    ) -> Result<soopy::TrackedStateResult> {
        self.worktrees
            .get_mut(worktree)
            .with_context(|| format!("Git worktree {} is not registered", worktree.0))?
            .tracked_state_with_metrics(query)
    }

    pub fn read_each<F>(
        &mut self,
        requests: &[soopy::ReadRequest],
        buffer: &mut Vec<u8>,
        mut visit: F,
    ) -> Result<()>
    where
        F: FnMut(soopy::SourceBytesRef<'_>) -> Result<()>,
    {
        let mut worktree_reads = BTreeMap::<soopy::WorktreeId, Vec<soopy::ReadRequest>>::new();
        let mut commit_reads = BTreeMap::<soopy::RepositoryId, Vec<soopy::ReadRequest>>::new();
        for request in requests {
            match &request.source.revision {
                soopy::RevisionId::Worktree { worktree, .. } => worktree_reads
                    .entry(worktree.clone())
                    .or_default()
                    .push(request.clone()),
                soopy::RevisionId::Commit(_) => commit_reads
                    .entry(request.source.repository.clone())
                    .or_default()
                    .push(request.clone()),
            }
        }
        for (worktree, reads) in worktree_reads {
            self.worktrees
                .get_mut(&worktree)
                .with_context(|| format!("Git worktree {} is not registered", worktree.0))?
                .source_tree()
                .read_each(&reads, buffer, &mut visit)?;
        }
        for (repository, reads) in commit_reads {
            let worktree = self
                .repositories
                .get(&repository)
                .and_then(|worktrees| worktrees.first())
                .with_context(|| format!("Git repository {} is not registered", repository.0))?
                .clone();
            self.worktrees
                .get_mut(&worktree)
                .context("registered Git worktree is missing")?
                .source_tree()
                .read_each(&reads, buffer, &mut visit)?;
        }
        Ok(())
    }
}
