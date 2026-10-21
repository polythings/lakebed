//! Apache Iceberg table metadata, manifests and commits, with no I/O of its own: everything is read
//! and written through a [`Host`](lakekit_core::Host) the application implements, so the crate
//! builds for `wasm32-unknown-unknown` (browsers, Cloudflare Workers) as well as native targets.
//!
//! - Reading (always): table metadata of format versions 1 to 3 ([`Metadata`]), schemas and
//!   partition specs, manifest lists and manifests with data and delete files ([`read_manifest`]),
//!   column and partition values ([`Datum`]), and loading a table from its location ([`Table`]).
//! - `write`: format v2 manifests and manifest lists, metadata updates, and the commit protocol of
//!   file-system tables (`commit`).
//! - `arrow`: the schema as an Arrow schema with field ids.
//! - `parquet-stats`: a data file's Iceberg statistics from its Parquet footer.
//!
//! ```
//! use lakekit_iceberg::{Datum, PrimitiveType};
//!
//! let bound = Datum::from_bytes(&PrimitiveType::Long, &42i64.to_le_bytes())?;
//! assert_eq!(bound, Datum::Long(42));
//! # Ok::<(), lakekit_core::Error>(())
//! ```

#[cfg(feature = "arrow")]
pub mod arrow;
mod avro;
#[cfg(feature = "write")]
pub mod commit;
mod datum;
pub mod manifest;
pub mod metadata;
mod partition;
#[cfg(feature = "parquet-stats")]
pub mod stats;
pub mod table;
mod types;

#[cfg(feature = "write")]
pub use commit::{Change, CommitHooks, CommitInfo, CommitOptions, Extra, NoHooks, commit};
pub use datum::{Datum, from_be, minimal_be};
pub use lakekit_core::{Error, Result};
pub use manifest::{
    Content, DataFile, Entry, FieldSummary, ManifestContent, ManifestFile, Status, read_manifest, read_manifest_list,
};
pub use metadata::{Metadata, Snapshot};
pub use partition::{PartitionField, PartitionFieldType, PartitionSpec, Transform};
pub use table::Table;
pub use types::{NestedField, PrimitiveType, Schema, Type};

/// Snapshot summary keys of Iceberg's standard counters.
pub mod summary {
    /// Data files added by the snapshot.
    pub const ADDED_DATA_FILES: &str = "added-data-files";
    /// Rows added by the snapshot.
    pub const ADDED_RECORDS: &str = "added-records";
    /// Bytes added by the snapshot.
    pub const ADDED_FILES_SIZE: &str = "added-files-size";
    /// Data files removed by the snapshot.
    pub const DELETED_DATA_FILES: &str = "deleted-data-files";
    /// Rows removed by the snapshot.
    pub const DELETED_RECORDS: &str = "deleted-records";
    /// Bytes removed by the snapshot.
    pub const REMOVED_FILES_SIZE: &str = "removed-files-size";
    /// Data files in the table after the snapshot.
    pub const TOTAL_DATA_FILES: &str = "total-data-files";
    /// Rows in the table after the snapshot.
    pub const TOTAL_RECORDS: &str = "total-records";
    /// Bytes in the table after the snapshot.
    pub const TOTAL_FILES_SIZE: &str = "total-files-size";
    /// Delete files in the table.
    pub const TOTAL_DELETE_FILES: &str = "total-delete-files";
    /// Position deletes in the table.
    pub const TOTAL_POSITION_DELETES: &str = "total-position-deletes";
    /// Equality deletes in the table.
    pub const TOTAL_EQUALITY_DELETES: &str = "total-equality-deletes";
}

fn corrupt(msg: impl Into<String>) -> Error {
    Error::Corrupt(msg.into())
}

/// Runs a future whose host calls complete at once ([`lakekit_core::MemoryHost`]'s do).
#[cfg(test)]
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let mut f = std::pin::pin!(f);
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}
