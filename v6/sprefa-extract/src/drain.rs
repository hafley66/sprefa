//! ast-grep edits drained into soopy source actions: `Edit<String>` ->
//! `soopy::TextEdit`, folded into the ONE Replace soopy takes per source file.

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use ast_grep_core::replacer::Replacer;
use ast_grep_core::source::{Doc, Edit};
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_core::{Matcher, Node};

pub use crate::types::{BoundEdit, PendingReplaceDoc};

/// `Edit<String>` carries `Underlying = u8` (`source.rs:151-160`), so position
/// and deleted_length are byte offsets that need no re-encoding.
impl From<BoundEdit> for soopy::TextEdit {
    fn from(bound: BoundEdit) -> Self {
        let start = bound.edit.position as u64;
        soopy::TextEdit {
            range: soopy::ActionSpan {
                source: bound.source,
                start,
                end: start + bound.edit.deleted_length as u64,
            },
            replacement: bound.edit.inserted_text,
            producer: bound.producer,
        }
    }
}

impl<L: LanguageExt> Doc for PendingReplaceDoc<L> {
    type Source = String;
    type Lang = L;
    type Node<'r> = tree_sitter::Node<'r>;

    fn get_lang(&self) -> &Self::Lang {
        self.lang()
    }

    fn get_source(&self) -> &String {
        self.source_text()
    }

    /// Appending never fails; a bad span surfaces at stage time as a soopy
    /// `MutationConflict`, where the whole batch is visible.
    fn do_edit(&mut self, edit: &Edit<String>) -> Result<(), String> {
        self.append(edit);
        Ok(())
    }

    fn root_node(&self) -> Self::Node<'_> {
        self.tree().root_node()
    }

    fn get_node_text<'a>(&'a self, node: &Self::Node<'a>) -> Cow<'a, str> {
        Cow::Borrowed(node.utf8_text(self.source_text().as_bytes()).unwrap_or(""))
    }
}

/// `Node::replace_all` (tree_sitter/mod.rs:439-450) is `StrDoc<L>`-only; this is
/// that walk generic over `D: Doc`, non-reentrant so the spans never overlap.
pub fn drain_edits<D, M, R>(root: &Node<'_, D>, matcher: &M, replacer: &R) -> Vec<Edit<D::Source>>
where
    D: Doc,
    M: Matcher,
    R: Replacer<D>,
{
    let mut edits = Vec::new();
    let mut consumed_end = 0usize;
    for matched in root.find_all(matcher) {
        let range = matched.range();
        if !edits.is_empty() && range.start < consumed_end {
            continue;
        }
        consumed_end = range.end;
        edits.push(matched.make_edit(matcher, replacer));
    }
    edits
}

/// One file's edits folded into the single Replace soopy takes per source.
/// Sorted and deduped by (start, end): two matchers on one span are one edit.
pub fn replace_action(
    source: soopy::ActionSource,
    expected: soopy::ContentId,
    mut edits: Vec<soopy::TextEdit>,
) -> soopy::SourceAction {
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    edits.dedup_by_key(|edit| (edit.range.start, edit.range.end));
    soopy::SourceAction::Replace {
        source,
        expected,
        edits,
    }
}

/// The staged request for one file's drained edits.
pub fn stage_edits(
    source: soopy::ActionSource,
    expected: soopy::ContentId,
    edits: Vec<soopy::TextEdit>,
    root_id: soopy::SourceRootId,
) -> soopy::StageRequest {
    soopy::StageRequest::new(root_id, vec![replace_action(source, expected, edits)])
}

/// A root-relative file in a plain-directory source root.
pub fn directory_source(identity: &soopy::DirectoryId, rel: &str) -> soopy::ActionSource {
    soopy::ActionSource::Directory {
        file: soopy::FileRef {
            directory: identity.clone(),
            path: soopy::RootPath(Arc::from(rel)),
        },
    }
}

/// A root-relative destination path in a plain-directory source root.
pub fn directory_path(rel: &str) -> soopy::SourcePath {
    soopy::SourcePath::Directory {
        path: soopy::RootPath(Arc::from(rel)),
    }
}

/// The root-relative path an action reads from, or None for a Create.
pub fn source_rel(action: &soopy::SourceAction) -> Option<&str> {
    let source = match action {
        soopy::SourceAction::Create { .. } => return None,
        soopy::SourceAction::Replace { source, .. }
        | soopy::SourceAction::Move { source, .. }
        | soopy::SourceAction::Delete { source, .. } => source,
    };
    match source {
        soopy::ActionSource::Directory { file } => Some(&file.path.0),
        soopy::ActionSource::Git { .. } => None,
    }
}

/// Re-aim a planned action at the root staging it: `DirectoryId` is blake3 of
/// the canonical root path. A Replace keeps the `expected` its edits were cut
/// against; a Move re-reads, since an earlier stage may have edited it.
pub fn bind_action(
    root: &Path,
    identity: &soopy::DirectoryId,
    action: &soopy::SourceAction,
) -> Result<soopy::SourceAction, String> {
    let expected = |rel: &str| -> Result<soopy::ContentId, String> {
        let path = root.join(rel);
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        Ok(soopy::ContentId::blake3(&bytes))
    };
    let Some(rel) = source_rel(action) else {
        return Ok(action.clone());
    };
    let source = directory_source(identity, rel);
    Ok(match action {
        soopy::SourceAction::Create { .. } => action.clone(),
        soopy::SourceAction::Delete { .. } => soopy::SourceAction::Delete {
            expected: expected(rel)?,
            source,
        },
        soopy::SourceAction::Move { destination, .. } => soopy::SourceAction::Move {
            expected: expected(rel)?,
            source,
            destination: destination.clone(),
        },
        soopy::SourceAction::Replace {
            edits, expected, ..
        } => soopy::SourceAction::Replace {
            expected: expected.clone(),
            edits: edits
                .iter()
                .map(|edit| soopy::TextEdit {
                    range: soopy::ActionSpan {
                        source: source.clone(),
                        start: edit.range.start,
                        end: edit.range.end,
                    },
                    replacement: edit.replacement.clone(),
                    producer: edit.producer.clone(),
                })
                .collect(),
            source,
        },
    })
}
