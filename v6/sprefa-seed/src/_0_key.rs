//! Identity — dense, normalized. Every id is a u32 surrogate; the repo/rev/file/
//! line hierarchy is a FK chain among these tables, NEVER repeated on a fact.
//! `x.file.rev` is surface sugar; it lowers to joins through these ids (see
//! lang::analyze / lang::lower). This is why "duplicate the piss out of cols" is
//! structurally impossible: a fact stores an id, not a coordinate.

macro_rules! dense_id {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $name(pub u32);
    )*};
}

dense_id!(
    SymId,   // interned string / any dl name (Souffle-style record interning)  [rename pending]
    RepoId, RevId, FileId, LocId,   // the normalized coordinate chain
    RelId, ColId, VarId, FieldId,   // program identities
);

/// The normalization/containment tables (owned by the store; keys live here).
/// A `loc` always resolves through file -> rev -> repo (total containment =
/// referential integrity = "file/rev/repo always appear together").
pub struct Loc  { pub file: FileId, pub line: u32, pub col: u32 }
pub struct File { pub rev: RevId, pub path: SymId }
pub struct Rev  { pub repo: RepoId, pub sha: SymId }
pub struct Repo { pub origin: SymId }
