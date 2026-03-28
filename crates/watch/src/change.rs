// ── raw FS events after debounce + correlation ───────────────────────────────

/// A semantic filesystem change, classified from raw notify events.
/// The watcher debounces raw events (~100ms), then correlates
/// delete+create pairs by content_hash to detect moves.
#[derive(Debug, Clone)]
pub enum FsChange {
    /// File moved/renamed. content_hash matched across delete+create.
    Move {
        file_id: i64,
        old_path: String,
        new_path: String,
    },
    /// File deleted with no matching create in the debounce window.
    Delete {
        file_id: i64,
        path: String,
    },
    /// New file appeared with no matching delete.
    Create {
        path: String,
    },
    /// File content changed (same path, different content_hash).
    ContentChange {
        file_id: i64,
        path: String,
    },
}

// ── ref-level changes derived from re-extraction ─────────────────────────────

/// A declaration-level change found by diffing old refs against new refs
/// for a ContentChange file.
#[derive(Debug, Clone)]
pub enum DeclChange {
    /// Same kind + same approximate span, different value.
    /// e.g. `export const Foo` became `export const Bar` at the same position.
    Rename {
        file_id: i64,
        kind: String,
        old_name: String,
        new_name: String,
        /// The span of the declaration in the new file content.
        new_span_start: u32,
        new_span_end: u32,
    },
    /// A declaration was added (no matching old ref by span proximity).
    Added {
        file_id: i64,
        kind: String,
        name: String,
    },
    /// A declaration was removed (no matching new ref by span proximity).
    Removed {
        file_id: i64,
        kind: String,
        name: String,
    },
}

// ── the unified change type fed into the planner ─────────────────────────────

/// Everything the rewrite planner needs to see.
#[derive(Debug, Clone)]
pub enum Change {
    Fs(FsChange),
    Decl(DeclChange),
}

impl From<FsChange> for Change {
    fn from(c: FsChange) -> Self {
        Change::Fs(c)
    }
}

impl From<DeclChange> for Change {
    fn from(c: DeclChange) -> Self {
        Change::Decl(c)
    }
}
