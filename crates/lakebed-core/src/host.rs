use std::future::Future;

use crate::Result;

/// An object found by [`Host::list`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Absolute URI.
    pub uri: String,
    /// Size in bytes.
    pub size: u64,
}

/// What a table format needs from where it runs: an object store, a clock, randomness, and a way
/// to wait. Paths are absolute URIs, as table metadata records them.
///
/// The futures aren't required to be `Send`, so hosts can wrap JavaScript promises.
pub trait Host {
    /// A multipart upload in progress.
    type Upload: Upload;

    /// The object at `uri`, or `None` if there is none.
    fn get(&self, uri: &str) -> impl Future<Output = Result<Option<Vec<u8>>>>;
    /// `len` bytes of the object at `uri` from `start`.
    fn get_range(&self, uri: &str, start: u64, len: u64) -> impl Future<Output = Result<Vec<u8>>>;
    /// Whether an object exists at `uri`.
    fn exists(&self, uri: &str) -> impl Future<Output = Result<bool>>;
    /// Writes (or replaces) the object at `uri`.
    fn put(&self, uri: &str, bytes: Vec<u8>) -> impl Future<Output = Result<()>>;
    /// Writes the object at `uri` unless one exists; `false` if one did. The commit primitive.
    fn put_if_absent(&self, uri: &str, bytes: Vec<u8>) -> impl Future<Output = Result<bool>>;
    /// Starts a multipart upload to `uri`.
    fn upload(&self, uri: &str) -> impl Future<Output = Result<Self::Upload>>;
    /// Objects whose URI starts with `prefix`.
    fn list(&self, prefix: &str) -> impl Future<Output = Result<Vec<Listed>>>;
    /// Deletes objects; missing ones are not an error.
    fn delete(&self, uris: &[String]) -> impl Future<Output = Result<()>>;
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> i64;
    /// A random number.
    fn random(&self) -> u64;
    /// Waits `ms` milliseconds.
    fn sleep(&self, ms: u64) -> impl Future<Output = ()>;
}

/// A multipart upload: parts in order, then [`Upload::complete`].
pub trait Upload {
    /// Sends the next part. Every part but the last must be at least the store's minimum (5 MiB on
    /// S3 and R2).
    fn put_part(&mut self, bytes: Vec<u8>) -> impl Future<Output = Result<()>>;
    /// Completes the upload: the object appears.
    fn complete(self) -> impl Future<Output = Result<()>>;
    /// Drops the parts sent.
    fn abort(self) -> impl Future<Output = ()>;
}
