//! A table at one version, found from its location in a store: metadata files are
//! `<location>/metadata/v<N>.metadata.json`, as file-system ("Hadoop") tables and Timelake name
//! them, with `version-hint.text` naming the newest for stores that can't list. Tables a catalog
//! names with other file names load with [`Table::from_metadata_file`].

use lakekit_core::Host;

use crate::manifest::{Entry, ManifestFile, Status, read_manifest, read_manifest_list};
use crate::metadata::Metadata;
use crate::{Error, Result};

/// `<location>/metadata/v<version>.metadata.json`.
pub fn metadata_uri(location: &str, version: u64) -> String {
    format!("{location}/metadata/v{version}.metadata.json")
}

/// `<location>/metadata/version-hint.text`.
pub fn hint_uri(location: &str) -> String {
    format!("{location}/metadata/version-hint.text")
}

/// The version in a metadata file's name: `v12.metadata.json` and `00012-<uuid>.metadata.json`
/// are version 12.
pub fn metadata_version(uri: &str) -> Option<u64> {
    let name = uri.rsplit('/').next()?.strip_suffix(".metadata.json")?;
    match name.strip_prefix('v') {
        Some(n) => n.parse().ok(),
        None => name.split_once('-')?.0.parse().ok(),
    }
}

/// A table at one committed version.
#[derive(Clone, Debug)]
pub struct Table {
    /// The table's root URI, without a trailing slash.
    pub location: String,
    /// The metadata version.
    pub version: u64,
    /// The metadata file this version was read from (the metadata log records it once a commit
    /// replaces it).
    pub metadata_file: String,
    /// The metadata.
    pub metadata: Metadata,
    /// The live files (data and delete files), when known: read by [`Table::live_files`], or
    /// derived by a commit.
    pub files: Option<Vec<Entry>>,
}

impl Table {
    /// A table at `version` with `metadata` from `v<version>.metadata.json`, its files not read
    /// yet.
    pub fn new(location: impl Into<String>, version: u64, metadata: Metadata) -> Self {
        let location = location.into();
        Self { metadata_file: metadata_uri(&location, version), location, version, metadata, files: None }
    }

    /// The newest metadata version of the table at `location`: the highest listed, or what
    /// `version-hint.text` says where nothing is listed.
    pub async fn latest_version<H: Host>(host: &H, location: &str) -> Result<u64> {
        let listed = host.list(&format!("{location}/metadata/v")).await?;
        if let Some(newest) = listed.iter().filter_map(|o| metadata_version(&o.uri)).max() {
            return Ok(newest);
        }
        let hint = host.get(&hint_uri(location)).await?.ok_or_else(|| Error::NotFound(location.to_string()))?;
        std::str::from_utf8(&hint)
            .ok()
            .and_then(|h| h.trim().parse().ok())
            .ok_or_else(|| crate::corrupt(format!("{}: not a version", hint_uri(location))))
    }

    /// The newest version of the table at `location`.
    pub async fn load<H: Host>(host: &H, location: &str) -> Result<Self> {
        let version = Self::latest_version(host, location).await?;
        Self::load_version(host, location, version).await
    }

    /// Version `version` of the table at `location`.
    pub async fn load_version<H: Host>(host: &H, location: &str, version: u64) -> Result<Self> {
        let uri = metadata_uri(location, version);
        let bytes = host.get(&uri).await?.ok_or_else(|| Error::NotFound(uri.clone()))?;
        Ok(Self::new(location, version, Metadata::parse(&bytes)?))
    }

    /// The table whose metadata file is `uri` (a catalog's pointer), at the location the metadata
    /// records.
    pub async fn from_metadata_file<H: Host>(host: &H, uri: &str) -> Result<Self> {
        let bytes = host.get(uri).await?.ok_or_else(|| Error::NotFound(uri.to_string()))?;
        let metadata = Metadata::parse(&bytes)?;
        let location = metadata.location().trim_end_matches('/').to_string();
        let table = Self::new(location, metadata_version(uri).unwrap_or(0), metadata);
        Ok(Self { metadata_file: uri.to_string(), ..table })
    }

    /// The current snapshot's manifests.
    pub async fn manifests<H: Host>(&self, host: &H) -> Result<Vec<ManifestFile>> {
        let Some(list) = self.metadata.manifest_list() else {
            if self.metadata.current_snapshot().is_some() {
                return Err(Error::Unsupported("format v1 snapshots without a manifest list".into()));
            }
            return Ok(vec![]);
        };
        let bytes = host.get(&list).await?.ok_or_else(|| Error::NotFound(list.clone()))?;
        read_manifest_list(&bytes)
    }

    /// The entries of `manifest`.
    pub async fn manifest<H: Host>(host: &H, manifest: &ManifestFile) -> Result<Vec<Entry>> {
        let bytes = host.get(&manifest.path).await?.ok_or_else(|| Error::NotFound(manifest.path.clone()))?;
        read_manifest(manifest, &bytes)
    }

    /// The live files (not deleted by the current snapshot), read from the manifests unless known.
    pub async fn live_files<H: Host>(&mut self, host: &H) -> Result<&[Entry]> {
        if self.files.is_none() {
            let manifests = self.manifests(host).await?;
            let read = futures_util::future::try_join_all(manifests.iter().map(|m| Self::manifest(host, m))).await?;
            let files = read.into_iter().flatten().filter(|e| e.status != Status::Deleted).collect();
            self.files = Some(files);
        }
        Ok(self.files.as_deref().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use lakekit_core::MemoryHost;

    use super::*;
    use crate::block_on;

    #[test]
    fn metadata_file_names() {
        assert_eq!(metadata_version("s3://b/t/metadata/v12.metadata.json"), Some(12));
        assert_eq!(metadata_version("s3://b/t/metadata/00012-1b2c.metadata.json"), Some(12));
        assert_eq!(metadata_version("s3://b/t/metadata/version-hint.text"), None);
        assert_eq!(metadata_version("s3://b/t/metadata/vx.metadata.json"), None);
    }

    #[test]
    fn loads_the_newest_version() {
        let host = MemoryHost::new(0);
        let meta = |n: u8| format!(r#"{{"format-version": 2, "location": "memory://t", "last-sequence-number": {n}}}"#);
        block_on(async {
            assert!(matches!(Table::load(&host, "memory://t").await, Err(Error::NotFound(_))));
            // Nothing listed: the hint names the version.
            host.put(&hint_uri("memory://t"), b"2\n".to_vec()).await?;
            assert_eq!(Table::latest_version(&host, "memory://t").await?, 2);
            host.put(&metadata_uri("memory://t", 2), meta(2).into_bytes()).await?;
            assert_eq!(Table::load(&host, "memory://t").await?.version, 2);
            host.put(&metadata_uri("memory://t", 10), meta(10).into_bytes()).await?;
            let mut t = Table::load(&host, "memory://t").await?;
            assert_eq!((t.version, t.metadata.next_sequence_number()), (10, 11));
            assert!(t.live_files(&host).await?.is_empty(), "no snapshot, no files");
            let uri = "memory://t/metadata/00002-a.metadata.json";
            host.put(uri, meta(2).into_bytes()).await?;
            let t = Table::from_metadata_file(&host, uri).await?;
            assert_eq!((t.location.as_str(), t.version, t.metadata_file.as_str()), ("memory://t", 2, uri));
            Ok::<_, Error>(())
        })
        .unwrap();
    }
}
