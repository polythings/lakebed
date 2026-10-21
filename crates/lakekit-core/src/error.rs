/// An error reading or writing a table.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The request doesn't fit the table, or a value is out of range.
    #[error("invalid: {0}")]
    Invalid(String),
    /// The table uses something this version can read but not write, or not read at all.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Another writer changed the table in a way this commit can't build on.
    #[error("conflict: {0}")]
    Conflict(String),
    /// A commit whose outcome can't be told: its metadata may be in the table. Nothing it wrote
    /// may be deleted; load the table again to see whether it committed.
    #[error("commit outcome unknown: {0}")]
    CommitUnknown(String),
    /// An object the table needs is missing.
    #[error("not found: {0}")]
    NotFound(String),
    /// The store failed.
    #[error("store: {0}")]
    Store(String),
    /// A file isn't what its format says it should be.
    #[error("unexpected file contents: {0}")]
    Corrupt(String),
}

/// `Result` with [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;
