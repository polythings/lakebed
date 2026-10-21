//! A lakekit [`Store`] over an [`object_store`] client: S3, GCS, Azure, local files, memory, and
//! anything else that implements `ObjectStore`. Applications enable the backends they need on their
//! own `object_store` dependency and hand the client to [`ObjectStoreAdapter::new`], or build one
//! from a URL with [`ObjectStoreAdapter::from_url`].
//!
//! lakekit names objects by absolute URI (`s3://bucket/warehouse/t/metadata/v1.metadata.json`);
//! the adapter maps them to paths under its root (`s3://bucket`).
//!
//! ```
//! use std::sync::Arc;
//!
//! use lakekit_object_store::ObjectStoreAdapter;
//! use object_store::memory::InMemory;
//!
//! let store = ObjectStoreAdapter::new(Arc::new(InMemory::new()), "memory://");
//! ```

use std::future::Future;
use std::sync::Arc;

use futures_util::TryStreamExt;
use lakekit_core::{Error, Listed, Result, Store, Upload};
use object_store::path::Path;
use object_store::{MultipartUpload, ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload};
use url::Url;

/// A [`Store`] over an `object_store` client whose objects have URIs under `root`.
#[derive(Clone, Debug)]
pub struct ObjectStoreAdapter {
    store: Arc<dyn ObjectStore>,
    root: String,
}

fn store_error(e: object_store::Error) -> Error {
    match e {
        object_store::Error::NotFound { path, .. } => Error::NotFound(path),
        object_store::Error::NotImplemented { operation, implementer } => {
            Error::Unsupported(format!("{implementer} has no {operation}"))
        }
        other => Error::Store(other.to_string()),
    }
}

impl ObjectStoreAdapter {
    /// `store`, whose objects have URIs under `root`: the object at path `a/b` is `<root>/a/b`. The
    /// root is the store's scheme and bucket (`s3://bucket`, `gs://bucket`, `memory://`), or
    /// `file:///` for a local file system rooted at `/`.
    pub fn new(store: Arc<dyn ObjectStore>, root: impl Into<String>) -> Self {
        let mut root = root.into();
        if !root.ends_with('/') {
            root.push('/');
        }
        Self { store, root }
    }

    /// The store for `url`'s scheme and bucket (`s3://bucket/…`), configured with `options`
    /// (`object_store`'s keys, such as `aws_region`). Its backend must be enabled on
    /// `object_store`.
    pub fn from_url<I, K, V>(url: &str, options: I) -> Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        let parsed = Url::parse(url).map_err(|e| Error::Invalid(format!("{url}: {e}")))?;
        let root = match parsed.host_str().filter(|h| !h.is_empty()) {
            Some(host) => format!("{}://{host}/", parsed.scheme()),
            // `file:///data/t`: a file system rooted at `/`.
            None => format!("{}:///", parsed.scheme()),
        };
        let root_url = Url::parse(&root).map_err(|e| Error::Invalid(format!("{root}: {e}")))?;
        let (store, _) = object_store::parse_url_opts(&root_url, options).map_err(store_error)?;
        Ok(Self::new(Arc::from(store), root))
    }

    /// The path of the object at `uri`.
    fn path(&self, uri: &str) -> Result<Path> {
        let rest = uri
            .strip_prefix(&self.root)
            .ok_or_else(|| Error::Invalid(format!("{uri} is outside this store ({})", self.root)))?;
        Path::parse(rest).map_err(|e| Error::Invalid(format!("{uri}: {e}")))
    }

    /// The URI of the object at `path`.
    fn uri(&self, path: &Path) -> String {
        format!("{}{path}", self.root)
    }
}

/// A multipart upload through `object_store`.
#[derive(Debug)]
pub struct ObjectStoreUpload(Box<dyn MultipartUpload>);

impl Upload for ObjectStoreUpload {
    async fn put_part(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.0.put_part(PutPayload::from(bytes)).await.map_err(store_error)
    }

    async fn complete(mut self) -> Result<()> {
        self.0.complete().await.map(drop).map_err(store_error)
    }

    async fn abort(mut self) {
        let _ = self.0.abort().await;
    }
}

impl Store for ObjectStoreAdapter {
    type Upload = ObjectStoreUpload;

    async fn get(&self, uri: &str) -> Result<Option<Vec<u8>>> {
        match self.store.get(&self.path(uri)?).await {
            Ok(found) => Ok(Some(found.bytes().await.map_err(store_error)?.to_vec())),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(store_error(e)),
        }
    }

    async fn get_range(&self, uri: &str, start: u64, len: u64) -> Result<Vec<u8>> {
        let range = start..start.saturating_add(len);
        Ok(self.store.get_range(&self.path(uri)?, range).await.map_err(store_error)?.to_vec())
    }

    async fn exists(&self, uri: &str) -> Result<bool> {
        match self.store.head(&self.path(uri)?).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(store_error(e)),
        }
    }

    async fn put(&self, uri: &str, bytes: Vec<u8>) -> Result<()> {
        self.store.put(&self.path(uri)?, PutPayload::from(bytes)).await.map(drop).map_err(store_error)
    }

    async fn put_if_absent(&self, uri: &str, bytes: Vec<u8>) -> Result<bool> {
        let options = PutOptions { mode: PutMode::Create, ..PutOptions::default() };
        match self.store.put_opts(&self.path(uri)?, PutPayload::from(bytes), options).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::AlreadyExists { .. } | object_store::Error::Precondition { .. }) => Ok(false),
            Err(e) => Err(store_error(e)),
        }
    }

    async fn upload(&self, uri: &str) -> Result<ObjectStoreUpload> {
        Ok(ObjectStoreUpload(self.store.put_multipart(&self.path(uri)?).await.map_err(store_error)?))
    }

    fn list(&self, prefix: &str) -> impl Future<Output = Result<Vec<Listed>>> {
        // `object_store` lists by directory; lakekit by string prefix (`…/metadata/v`). List the
        // directory the prefix is in and keep what starts with it.
        let relative = prefix.strip_prefix(&self.root).map(|rest| rest.rsplit_once('/').map_or("", |(dir, _)| dir));
        async move {
            let relative =
                relative.ok_or_else(|| Error::Invalid(format!("{prefix} is outside this store ({})", self.root)))?;
            let dir = Path::parse(relative).map_err(|e| Error::Invalid(format!("{prefix}: {e}")))?;
            let dir = (!relative.is_empty()).then_some(&dir);
            let objects: Vec<_> = self.store.list(dir).try_collect().await.map_err(store_error)?;
            Ok(objects
                .into_iter()
                .map(|o| Listed { uri: self.uri(&o.location), size: o.size })
                .filter(|o| o.uri.starts_with(prefix))
                .collect())
        }
    }

    async fn delete(&self, uris: &[String]) -> Result<()> {
        let deletes = uris.iter().map(|uri| async move {
            match self.store.delete(&self.path(uri)?).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
                Err(e) => Err(store_error(e)),
            }
        });
        futures_util::future::try_join_all(deletes).await.map(drop)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::collections::HashMap;

    use lakekit_core::{TokioEnv, WithEnv};
    use lakekit_iceberg::{Change, CommitOptions, DataFile, Datum, Metadata, NoHooks, Table, commit};
    use object_store::memory::InMemory;

    use super::*;

    fn adapter() -> ObjectStoreAdapter {
        ObjectStoreAdapter::new(Arc::new(InMemory::new()), "memory://")
    }

    #[tokio::test]
    async fn objects_round_trip() {
        let store = adapter();
        assert_eq!(store.get("memory://t/a").await.unwrap(), None);
        assert!(!store.exists("memory://t/a").await.unwrap());
        store.put("memory://t/a", b"hello".to_vec()).await.unwrap();
        assert_eq!(store.get("memory://t/a").await.unwrap().as_deref(), Some(&b"hello"[..]));
        assert_eq!(store.get_range("memory://t/a", 1, 3).await.unwrap(), b"ell");
        assert!(store.put_if_absent("memory://t/b", b"1".to_vec()).await.unwrap());
        assert!(!store.put_if_absent("memory://t/b", b"2".to_vec()).await.unwrap(), "the first write wins");
        assert_eq!(store.get("memory://t/b").await.unwrap().as_deref(), Some(&b"1"[..]));

        let mut upload = store.upload("memory://t/c").await.unwrap();
        upload.put_part(b"ab".to_vec()).await.unwrap();
        upload.put_part(b"cd".to_vec()).await.unwrap();
        upload.complete().await.unwrap();
        assert_eq!(store.get("memory://t/c").await.unwrap().as_deref(), Some(&b"abcd"[..]));

        store.delete(&["memory://t/a".into(), "memory://t/missing".into()]).await.unwrap();
        assert!(!store.exists("memory://t/a").await.unwrap());
        assert!(matches!(store.get("s3://elsewhere/x").await, Err(Error::Invalid(_))));
    }

    #[tokio::test]
    async fn lists_by_string_prefix() {
        let store = adapter();
        for name in ["v1.metadata.json", "v2.metadata.json", "version-hint.text", "snap-1.avro"] {
            store.put(&format!("memory://t/metadata/{name}"), vec![0; 3]).await.unwrap();
        }
        store.put("memory://t/data/v9.parquet", vec![]).await.unwrap();
        let mut listed: Vec<String> =
            store.list("memory://t/metadata/v").await.unwrap().into_iter().map(|o| o.uri).collect();
        listed.sort();
        assert_eq!(
            listed,
            [
                "memory://t/metadata/v1.metadata.json",
                "memory://t/metadata/v2.metadata.json",
                "memory://t/metadata/version-hint.text"
            ]
        );
        assert_eq!(store.list("memory://t/metadata/s").await.unwrap()[0].size, 3);
        assert_eq!(store.list("memory://t").await.unwrap().len(), 5, "a prefix right under the root");
    }

    #[tokio::test]
    async fn local_files_have_file_uris() {
        let dir = tempfile::tempdir().unwrap();
        let store = ObjectStoreAdapter::from_url("file:///", std::iter::empty::<(&str, String)>()).unwrap();
        let uri = format!("file://{}/t/a.json", dir.path().display());
        store.put(&uri, b"{}".to_vec()).await.unwrap();
        assert!(dir.path().join("t/a.json").exists());
        let listed = store.list(&format!("file://{}/t/", dir.path().display())).await.unwrap();
        assert_eq!(listed[0].uri, uri);
    }

    /// An Iceberg table committed and read back through `object_store`.
    #[tokio::test]
    async fn iceberg_commits_through_object_store() {
        let store = WithEnv::new(adapter(), TokioEnv);
        let location = "memory://warehouse/t";
        let metadata = Metadata::parse(
            br#"{"format-version": 2, "table-uuid": "u", "location": "memory://warehouse/t", "last-sequence-number": 0,
                "last-updated-ms": 0, "last-column-id": 1, "current-schema-id": 0,
                "schemas": [{"schema-id": 0, "type": "struct", "fields": [{"id": 1, "name": "x", "required": true, "type": "long"}]}],
                "default-spec-id": 0, "partition-specs": [{"spec-id": 0, "fields": []}], "last-partition-id": 999,
                "default-sort-order-id": 0, "sort-orders": [{"order-id": 0, "fields": []}], "properties": {},
                "current-snapshot-id": -1, "snapshots": [], "snapshot-log": [], "metadata-log": []}"#,
        )
        .unwrap();
        let table = Table::new(location, 1, metadata);
        store.put(&table.metadata_file, table.metadata.to_json().into_bytes()).await.unwrap();
        let file = DataFile {
            path: format!("{location}/data/a.parquet"),
            partition: Vec::<Option<Datum>>::new(),
            record_count: 3,
            file_size: 30,
            ..DataFile::default()
        };
        let change = Change::Append { files: vec![file] };
        let (_, info) =
            commit(&store, table, &change, &HashMap::new(), &CommitOptions::default(), &mut NoHooks).await.unwrap();
        assert_eq!(info.version, 2);

        let mut loaded = Table::load(&store, location).await.unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.live_files(&store).await.unwrap()[0].file.record_count, 3);
    }
}
