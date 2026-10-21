//! Commits to file-system tables (feature `write`): every file of a snapshot (manifests, manifest
//! list, and whatever the caller adds) is uploaded, then `v<N+1>.metadata.json` is written only if
//! absent. When another writer got there first, the commit reloads the table and tries again on top
//! of it. `version-hint.text` is written after the commit; readers don't rely on it.
//!
//! Applications extend a commit through [`CommitHooks`]: files of their own written with the
//! snapshot, summary properties, a check that skips a commit already made, and how to reload.

use core::time::Duration;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::hash::BuildHasher;
use std::sync::Arc;

use lakekit_core::{Clock, Random, Rng, Services, Store, uuid_v4};

use crate::manifest::{
    DataFile, Entry, ManifestContext, ManifestFile, ManifestWrite, Status, manifest_file, write_manifest,
    write_manifest_list,
};
use crate::metadata::NewSnapshot;
use crate::table::{Table, hint_uri, metadata_uri};
use crate::{Error, Result, summary};

/// What a commit changes.
#[derive(Clone, Debug)]
pub enum Change {
    /// Add data files.
    Append {
        /// The files.
        files: Vec<DataFile>,
    },
    /// Remove files and add files with the same rows (compaction). Each added file keeps the data
    /// sequence number in `sequence_numbers`, so its rows keep their order against other files'.
    Replace {
        /// Files removed.
        removed: Vec<Entry>,
        /// Files added.
        added: Vec<DataFile>,
        /// Data sequence number of each added file.
        sequence_numbers: Vec<i64>,
    },
    /// Remove files (retention).
    Delete {
        /// Files removed.
        removed: Vec<Entry>,
    },
}

impl Change {
    /// Files the change adds.
    pub fn added(&self) -> &[DataFile] {
        match self {
            Self::Append { files } => files,
            Self::Replace { added, .. } => added,
            Self::Delete { .. } => &[],
        }
    }

    /// Files the change removes.
    pub fn removed(&self) -> &[Entry] {
        match self {
            Self::Append { .. } => &[],
            Self::Replace { removed, .. } | Self::Delete { removed } => removed,
        }
    }

    fn data_sequence(&self, i: usize) -> Option<i64> {
        match self {
            Self::Replace { sequence_numbers, .. } => sequence_numbers.get(i).copied(),
            _ => None,
        }
    }

    /// The snapshot operation: `append`, `replace` or `delete`.
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Append { .. } => "append",
            Self::Replace { .. } => "replace",
            Self::Delete { .. } => "delete",
        }
    }
}

/// How a commit behaves. The defaults are the ones Timelake's engine uses.
#[derive(Clone, Debug)]
pub struct CommitOptions {
    /// Attempts before a commit that keeps losing the race gives up with [`Error::Conflict`].
    pub max_attempts: u64,
    /// The wait before attempt `n` is a random number of milliseconds in this range, times `n`.
    pub backoff_ms: (u64, u64),
    /// An append merges the table's manifests into one once there would be more than this many.
    pub merge_manifests_at: usize,
    /// Write `version-hint.text` after each commit, for readers of stores that can't list.
    pub write_version_hint: bool,
}

impl Default for CommitOptions {
    fn default() -> Self {
        Self { max_attempts: 10, backoff_ms: (5, 50), merge_manifests_at: 8, write_version_hint: true }
    }
}

/// How a commit went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitInfo {
    /// The new snapshot (when skipped, the current one).
    pub snapshot_id: i64,
    /// The table version after the commit.
    pub version: u64,
    /// Attempts it took.
    pub attempts: u64,
    /// [`CommitHooks::skip`] found the change already committed; nothing was written.
    pub skipped: bool,
}

/// What an application adds to a snapshot.
#[derive(Clone, Debug, Default)]
pub struct Extra {
    /// Files to upload with the snapshot's manifests: `(uri, bytes)`. A commit that loses the race
    /// deletes them.
    pub files: Vec<(String, Vec<u8>)>,
    /// Summary properties. They override the caller's; Iceberg's counters override them.
    pub summary: HashMap<String, String>,
}

/// An application's part in a commit. Every method has a default that does nothing extra.
pub trait CommitHooks<S: Store, C: Clock, R: Random> {
    /// The error the hooks (and so the commit) return.
    type Error: From<Error> + fmt::Debug;

    /// The newest version of the table, after another writer won the race.
    fn reload(
        &mut self,
        services: Services<'_, S, C, R>,
        location: &str,
    ) -> impl Future<Output = Result<Table, Self::Error>> {
        async move { Ok(Table::load(services.store, location).await?) }
    }

    /// Whether `base` already holds this change (an idempotent append retried), so nothing is
    /// committed. Checked before every attempt.
    fn skip(&mut self, _base: &Table) -> bool {
        false
    }

    /// Files and summary properties to add to snapshot `snapshot_id` on top of `base`. Called once
    /// per attempt.
    fn extra(
        &mut self,
        _services: Services<'_, S, C, R>,
        _base: &Table,
        _snapshot_id: i64,
    ) -> impl Future<Output = Result<Extra, Self::Error>> {
        async { Ok(Extra::default()) }
    }
}

/// Hooks that add nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHooks;

impl<S: Store, C: Clock, R: Random> CommitHooks<S, C, R> for NoHooks {
    type Error = Error;
}

/// Commits `change` on top of `base` (or the newest version, after a lost race), with `props` in
/// the snapshot summary. The returned table knows its files when `base` did.
pub async fn commit<S: Store, C: Clock, R: Random, K: CommitHooks<S, C, R>, P: BuildHasher>(
    services: Services<'_, S, C, R>,
    base: Table,
    change: &Change,
    props: &HashMap<String, String, P>,
    options: &CommitOptions,
    hooks: &mut K,
) -> Result<(Table, CommitInfo), K::Error> {
    base.metadata.check_writable()?;
    if let Change::Replace { added, sequence_numbers, .. } = change
        && added.len() != sequence_numbers.len()
    {
        let (files, numbers) = (added.len(), sequence_numbers.len());
        return Err(Error::Invalid(format!("replace: {files} added files but {numbers} sequence numbers")).into());
    }
    let mut base = base;
    for attempt in 1..=options.max_attempts {
        if attempt > 1 {
            let (lo, hi) = options.backoff_ms;
            let ms = (lo + services.random.rng().next_u64() % hi.saturating_sub(lo).max(1)) * attempt;
            services.clock.sleep(Duration::from_millis(ms)).await;
            base = hooks.reload(services, &base.location).await?;
        }
        if hooks.skip(&base) {
            let info = CommitInfo {
                snapshot_id: base.metadata.current_snapshot_id().unwrap_or_default(),
                version: base.version,
                attempts: attempt,
                skipped: true,
            };
            return Ok((base, info));
        }
        if let Some((table, snapshot_id)) = attempt_commit(services, &base, change, props, options, hooks).await? {
            let info = CommitInfo { snapshot_id, version: table.version, attempts: attempt, skipped: false };
            return Ok((table, info));
        }
    }
    Err(Error::Conflict(format!("{}: gave up after {} attempts", base.location, options.max_attempts)).into())
}

/// Writes table metadata `bytes` at `uri` unless a version is there; `false` if another writer's
/// is.
///
/// A conditional PUT that reports failure may have landed all the same: a retry after a first try
/// that landed gets "already exists", and a timeout says nothing. The version is then read back:
/// our own metadata there is the commit, another writer's means it lost. If that can't be told,
/// the error is [`Error::CommitUnknown`], and callers must not delete what the metadata references.
pub async fn put_metadata<S: Store>(store: &S, uri: &str, bytes: Vec<u8>) -> Result<bool> {
    let unknown = |e: &dyn fmt::Display| Error::CommitUnknown(format!("{uri}: {e}"));
    match store.put_if_absent(uri, bytes.clone()).await {
        Ok(true) => Ok(true),
        // Ours (an earlier try landed) or another writer's. Absent now, it was never ours: our
        // version would still be there.
        Ok(false) => match store.get(uri).await {
            Ok(found) => Ok(found.is_some_and(|found| found == bytes)),
            Err(e) => Err(unknown(&e)),
        },
        // Absent now, the PUT may still land.
        Err(e) => match store.get(uri).await {
            Ok(Some(found)) => Ok(found == bytes),
            Ok(None) | Err(_) => Err(unknown(&e)),
        },
    }
}

/// A snapshot id not in the table yet.
fn new_snapshot_id(random: &impl Random, base: &Table) -> i64 {
    let mut rng = random.rng();
    loop {
        let id = i64::try_from(rng.next_u64() >> 1).unwrap_or(1).max(1);
        if !base.metadata.has_snapshot(id) {
            return id;
        }
    }
}

/// One attempt: writes the snapshot's files and publishes it. `None` if another writer committed
/// this version first (this attempt's files are deleted). With an unknown outcome, nothing is
/// deleted.
async fn attempt_commit<S: Store, C: Clock, R: Random, K: CommitHooks<S, C, R>, P: BuildHasher>(
    services: Services<'_, S, C, R>,
    base: &Table,
    change: &Change,
    props: &HashMap<String, String, P>,
    options: &CommitOptions,
    hooks: &mut K,
) -> Result<Option<(Table, i64)>, K::Error> {
    let snapshot_id = new_snapshot_id(services.random, base);
    let sequence_number = base.metadata.next_sequence_number();
    let mut uploads: Vec<(String, Vec<u8>)> = vec![];
    let mut unknown = false;
    let result = async {
        let manifests = write_manifests(services, base, change, snapshot_id, options, &mut uploads).await?;
        let extra = hooks.extra(services, base, snapshot_id).await?;
        uploads.extend(extra.files);
        let list_uri = format!("{}/metadata/snap-{snapshot_id}-{}.avro", base.location, uuid_v4(services.random));
        let parent = base.metadata.current_snapshot_id();
        let list = write_manifest_list(&manifests, snapshot_id, parent, sequence_number)?;
        uploads.push((list_uri.clone(), list));
        let puts = uploads.iter().map(|(uri, bytes)| services.store.put(uri, bytes.clone()));
        futures_util::future::try_join_all(puts).await.map_err(Error::from)?;

        let mut summary: HashMap<String, String> = props.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        summary.extend(extra.summary);
        summary.extend(counters(change, &base.metadata.summary()));
        // Snapshot and metadata logs must stay in time order, so a writer whose clock is behind the
        // last one's dates its commit at the table's last update.
        let now = services.clock.now_ms().max(base.metadata.last_updated_ms());
        let snapshot = NewSnapshot {
            snapshot_id,
            sequence_number,
            timestamp_ms: now,
            manifest_list: list_uri,
            operation: change.operation().into(),
            summary,
        };
        let metadata = base.metadata.with_snapshot(&snapshot, &base.metadata_file, now)?;
        let version = base.version + 1;
        let ours =
            put_metadata(services.store, &metadata_uri(&base.location, version), metadata.to_json().into_bytes())
                .await
                .inspect_err(|e| unknown = matches!(e, Error::CommitUnknown(_)))?;
        Ok::<_, K::Error>(ours.then_some((metadata, version)))
    }
    .await;
    let (metadata, version) = match result {
        Ok(Some(committed)) => committed,
        Ok(None) => {
            delete_uploads(services.store, uploads).await;
            return Ok(None);
        }
        Err(e) => {
            if !unknown {
                delete_uploads(services.store, uploads).await;
            }
            return Err(e);
        }
    };

    let files = base.files.as_ref().map(|files| {
        let removed: HashSet<&str> = change.removed().iter().map(|e| e.file.path.as_str()).collect();
        let mut next: Vec<Entry> = change
            .added()
            .iter()
            .enumerate()
            .map(|(i, f)| Entry {
                status: Status::Added,
                snapshot_id,
                sequence_number: change.data_sequence(i).unwrap_or(sequence_number),
                file_sequence_number: sequence_number,
                file: Arc::new(f.clone()),
            })
            .collect();
        next.extend(files.iter().filter(|e| !removed.contains(e.file.path.as_str())).cloned());
        next
    });
    let table = Table { files, ..Table::new(base.location.clone(), version, metadata) };
    if options.write_version_hint {
        let _ = services.store.put(&hint_uri(&table.location), version.to_string().into_bytes()).await;
    }
    Ok(Some((table, snapshot_id)))
}

async fn delete_uploads(store: &impl Store, uploads: Vec<(String, Vec<u8>)>) {
    let written: Vec<String> = uploads.into_iter().map(|(uri, _)| uri).collect();
    let _ = store.delete(&written).await;
}

/// Writes a manifest of the added files, carries the base snapshot's manifests over, and rewrites
/// the ones holding removed files (or all of them once there are too many).
async fn write_manifests<S: Store, C, R: Random>(
    services: Services<'_, S, C, R>,
    base: &Table,
    change: &Change,
    snapshot_id: i64,
    options: &CommitOptions,
    uploads: &mut Vec<(String, Vec<u8>)>,
) -> Result<Vec<ManifestFile>> {
    let metadata = &base.metadata;
    let schema_json = metadata.schema_json()?.to_string();
    let (spec_id, spec_fields) = metadata.partition_spec_json()?;
    let spec_fields_json = spec_fields.to_string();
    let partition_type = metadata.partition_type()?;
    let ctx = ManifestContext {
        schema_json: &schema_json,
        schema_id: metadata.schema_id(),
        spec_fields_json: &spec_fields_json,
        spec_id,
        partition_type: &partition_type,
    };
    let mut write = |kind: &str, entries: &[ManifestWrite]| -> Result<ManifestFile> {
        let uri = format!("{}/metadata/{}-{kind}-m.avro", base.location, uuid_v4(services.random));
        let bytes = write_manifest(&ctx, entries)?;
        let mf = manifest_file(&uri, &bytes, spec_id, snapshot_id, entries, &partition_type);
        uploads.push((uri, bytes));
        Ok(mf)
    };
    let mut new = vec![];
    let added = change.added();
    if !added.is_empty() {
        let entries: Vec<ManifestWrite> = added
            .iter()
            .enumerate()
            .map(|(i, f)| ManifestWrite {
                status: Status::Added,
                snapshot_id: Some(snapshot_id),
                sequence_number: change.data_sequence(i),
                file_sequence_number: None,
                file: Arc::new(f.clone()),
            })
            .collect();
        new.push(write("added", &entries)?);
    }

    let current = base.manifests(services.store).await?;
    let removed: HashSet<&str> = change.removed().iter().map(|e| e.file.path.as_str()).collect();
    let merge = removed.is_empty() && current.len() + new.len() > options.merge_manifests_at;
    if removed.is_empty() && !merge {
        new.extend(current);
        return Ok(new);
    }
    let loaded = futures_util::future::try_join_all(current.iter().map(|m| Table::manifest(services.store, m))).await?;
    let (mut kept, mut deleted, mut carried) = (vec![], vec![], vec![]);
    for (manifest, entries) in current.into_iter().zip(loaded) {
        let live: Vec<&Entry> = entries.iter().filter(|e| e.status != Status::Deleted).collect();
        let touched = live.iter().any(|e| removed.contains(e.file.path.as_str()));
        // Delete manifests and other specs' manifests are carried as they are.
        let mergeable = manifest.content == crate::ManifestContent::Data && manifest.partition_spec_id == spec_id;
        if !mergeable || (!merge && !touched) {
            if touched {
                return Err(Error::Unsupported(format!(
                    "{}: removing files of an older partition spec",
                    base.location
                )));
            }
            carried.push(manifest);
            continue;
        }
        for e in live {
            let (status, snapshot) = if removed.contains(e.file.path.as_str()) {
                (Status::Deleted, snapshot_id)
            } else {
                (Status::Existing, e.snapshot_id)
            };
            let w = ManifestWrite {
                status,
                snapshot_id: Some(snapshot),
                sequence_number: Some(e.sequence_number),
                file_sequence_number: Some(e.file_sequence_number),
                file: Arc::clone(&e.file),
            };
            if status == Status::Deleted { deleted.push(w) } else { kept.push(w) }
        }
    }
    if deleted.len() != removed.len() {
        return Err(Error::Conflict(format!(
            "{}: {} of {} files to remove are no longer in the table (concurrent maintenance?)",
            base.location,
            removed.len() - deleted.len(),
            removed.len()
        )));
    }
    if !kept.is_empty() {
        new.push(write("merged", &kept)?);
    }
    if !deleted.is_empty() {
        new.push(write("deleted", &deleted)?);
    }
    new.extend(carried);
    Ok(new)
}

/// Iceberg's standard snapshot summary counters after `change` on a table whose current summary is
/// `base`. Delete-file totals carry over: commits here don't add or remove delete files.
pub fn counters<S: BuildHasher>(change: &Change, base: &HashMap<String, String, S>) -> BTreeMap<String, String> {
    let total = |k: &str| base.get(k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let (added, removed) = (change.added(), change.removed());
    let added_rows: u64 = added.iter().map(|f| f.record_count).sum();
    let added_bytes: u64 = added.iter().map(|f| f.file_size).sum();
    let removed_rows: u64 = removed.iter().map(|e| e.file.record_count).sum();
    let removed_bytes: u64 = removed.iter().map(|e| e.file.file_size).sum();
    let (added_files, removed_files) = (added.len() as u64, removed.len() as u64);
    let mut out = BTreeMap::new();
    for (k, v) in [
        (summary::ADDED_DATA_FILES, added_files),
        (summary::ADDED_RECORDS, added_rows),
        (summary::ADDED_FILES_SIZE, added_bytes),
        (summary::DELETED_DATA_FILES, removed_files),
        (summary::DELETED_RECORDS, removed_rows),
        (summary::REMOVED_FILES_SIZE, removed_bytes),
    ] {
        if v != 0 {
            out.insert(k.to_string(), v.to_string());
        }
    }
    for (k, v) in [
        (summary::TOTAL_DATA_FILES, (total(summary::TOTAL_DATA_FILES) + added_files).saturating_sub(removed_files)),
        (summary::TOTAL_RECORDS, (total(summary::TOTAL_RECORDS) + added_rows).saturating_sub(removed_rows)),
        (summary::TOTAL_FILES_SIZE, (total(summary::TOTAL_FILES_SIZE) + added_bytes).saturating_sub(removed_bytes)),
        (summary::TOTAL_DELETE_FILES, total(summary::TOTAL_DELETE_FILES)),
        (summary::TOTAL_POSITION_DELETES, total(summary::TOTAL_POSITION_DELETES)),
        (summary::TOTAL_EQUALITY_DELETES, total(summary::TOTAL_EQUALITY_DELETES)),
    ] {
        out.insert(k.to_string(), v.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use lakekit_core::{MemoryStore, SeededRandom, TestClock};
    use serde_json::json;

    use super::*;
    use crate::{Datum, Metadata, block_on};

    const LOCATION: &str = "memory://wh/t";

    /// A store, a clock, and a random source in memory: what a test commit uses.
    struct Mem {
        store: MemoryStore,
        clock: TestClock,
        random: SeededRandom,
    }

    impl Mem {
        fn at_ms(ms: i64) -> Self {
            Self { store: MemoryStore::new(), clock: TestClock::at_ms(ms), random: SeededRandom::new(0) }
        }

        fn services(&self) -> Services<'_, MemoryStore, TestClock, SeededRandom> {
            Services::new(&self.store, &self.clock, &self.random)
        }
    }

    fn create(store: &Mem, format_version: u8) -> Table {
        let metadata = json!({
            "format-version": format_version, "table-uuid": "u", "location": LOCATION, "last-sequence-number": 0,
            "last-updated-ms": 0, "last-column-id": 2, "current-schema-id": 0,
            "schemas": [{"schema-id": 0, "type": "struct", "fields": [
                {"id": 1, "name": "time", "required": true, "type": "timestamptz"},
                {"id": 2, "name": "device", "required": true, "type": "string"}]}],
            "default-spec-id": 0, "last-partition-id": 1000,
            "partition-specs": [{"spec-id": 0, "fields": [
                {"source-id": 1, "field-id": 1000, "name": "time_day", "transform": "day"}]}],
            "default-sort-order-id": 0, "sort-orders": [{"order-id": 0, "fields": []}],
            "properties": {}, "current-snapshot-id": -1, "snapshots": [], "snapshot-log": [], "metadata-log": []
        });
        let table = Table::new(LOCATION, 1, Metadata::from_json(metadata).unwrap());
        block_on(store.store.put(&table.metadata_file, table.metadata.to_json().into_bytes())).unwrap();
        Table { files: Some(vec![]), ..table }
    }

    fn file(name: &str, day: i32, rows: u64) -> DataFile {
        DataFile {
            path: format!("{LOCATION}/data/{name}.parquet"),
            partition: vec![Some(Datum::Date(day))],
            record_count: rows,
            file_size: rows * 10,
            ..DataFile::default()
        }
    }

    fn append(files: Vec<DataFile>) -> Change {
        Change::Append { files }
    }

    fn try_run<K: CommitHooks<MemoryStore, TestClock, SeededRandom>>(
        store: &Mem,
        base: Table,
        change: &Change,
        hooks: &mut K,
    ) -> Result<(Table, CommitInfo), K::Error> {
        block_on(commit(store.services(), base, change, &HashMap::new(), &CommitOptions::default(), hooks))
    }

    fn run<K: CommitHooks<MemoryStore, TestClock, SeededRandom>>(
        store: &Mem,
        base: Table,
        change: &Change,
        hooks: &mut K,
    ) -> (Table, CommitInfo) {
        try_run(store, base, change, hooks).unwrap()
    }

    fn files_named(t: &Table, name: &str) -> Vec<Entry> {
        t.files.iter().flatten().filter(|e| e.file.path.ends_with(name)).cloned().collect()
    }

    /// The live files of the newest version, read back from the store: (name, data sequence
    /// number, file sequence number).
    fn live(store: &Mem) -> Vec<(String, i64, i64)> {
        let mut t = block_on(Table::load(&store.store, LOCATION)).unwrap();
        let mut files: Vec<_> = block_on(t.live_files(&store.store))
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e.file.path.rsplit('/').next().unwrap_or_default().to_string(),
                    e.sequence_number,
                    e.file_sequence_number,
                )
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn append_replace_and_delete() {
        let store = Mem::at_ms(1_000);
        let t = create(&store, 2);
        let (t, info) = run(&store, t, &append(vec![file("a", 1, 10), file("b", 2, 20)]), &mut NoHooks);
        assert_eq!((info.version, info.attempts, info.skipped), (2, 1, false));
        assert_eq!(live(&store), [("a.parquet".into(), 1, 1), ("b.parquet".into(), 1, 1)]);

        // Compaction keeps the data sequence number of what it replaces.
        let change = Change::Replace {
            removed: files_named(&t, "a.parquet"),
            added: vec![file("c", 1, 10)],
            sequence_numbers: vec![1],
        };
        let (t, _) = run(&store, t, &change, &mut NoHooks);
        assert_eq!(live(&store), [("b.parquet".into(), 1, 1), ("c.parquet".into(), 1, 2)]);
        assert_eq!(t.files.as_ref().map(Vec::len), Some(2), "the commit derives the files");

        let (t, _) = run(&store, t.clone(), &Change::Delete { removed: files_named(&t, "b.parquet") }, &mut NoHooks);
        assert_eq!(live(&store), [("c.parquet".into(), 1, 2)]);
        let summary = t.metadata.summary();
        assert_eq!((summary["operation"].as_str(), summary[crate::summary::TOTAL_RECORDS].as_str()), ("delete", "10"));
        assert_eq!(summary[crate::summary::DELETED_DATA_FILES], "1");
        assert_eq!(t.metadata.metadata_log_len(), 3);
        assert_eq!(block_on(store.store.get(&hint_uri(LOCATION))).unwrap().as_deref(), Some(&b"4"[..]));
    }

    #[test]
    fn a_lost_race_retries_on_the_newer_version() {
        let store = Mem::at_ms(1_000);
        let stale = create(&store, 2);
        let (_, first) = run(&store, stale.clone(), &append(vec![file("a", 1, 10)]), &mut NoHooks);
        let (_, second) = run(&store, stale, &append(vec![file("b", 1, 10)]), &mut NoHooks);
        assert_eq!((first.version, second.version, second.attempts), (2, 3, 2));
        assert_eq!(live(&store).len(), 2);
        // The lost attempt's manifests and list were deleted: every list is a snapshot's.
        let lists = store.store.uris().iter().filter(|u| u.contains("/snap-")).count();
        assert_eq!(lists, 2);
    }

    #[test]
    fn removing_files_that_are_gone_is_a_conflict() {
        let store = Mem::at_ms(1_000);
        let (t, _) = run(&store, create(&store, 2), &append(vec![file("a", 1, 10)]), &mut NoHooks);
        let removed = t.files.clone().unwrap_or_default();
        let (t, _) = run(&store, t, &Change::Delete { removed: removed.clone() }, &mut NoHooks);
        let err = try_run(&store, t, &Change::Delete { removed }, &mut NoHooks);
        assert!(matches!(err, Err(Error::Conflict(_))), "{err:?}");
    }

    /// Hooks that skip a batch already committed, and add a file and summary properties.
    struct Batch(&'static str);

    impl CommitHooks<MemoryStore, TestClock, SeededRandom> for Batch {
        type Error = Error;

        fn skip(&mut self, base: &Table) -> bool {
            base.metadata.ancestors().iter().any(|s| s.summary.get("batch").map(String::as_str) == Some(self.0))
        }

        fn extra(
            &mut self,
            _: Services<'_, MemoryStore, TestClock, SeededRandom>,
            base: &Table,
            snapshot_id: i64,
        ) -> impl Future<Output = Result<Extra>> {
            let uri = format!("{}/metadata/extra-{snapshot_id}", base.location);
            let summary =
                HashMap::from([("batch".to_string(), self.0.to_string()), ("extra".to_string(), uri.clone())]);
            std::future::ready(Ok(Extra { files: vec![(uri, b"x".to_vec())], summary }))
        }
    }

    #[test]
    fn hooks_skip_and_extend_commits() {
        let store = Mem::at_ms(1_000);
        let (t, info) = run(&store, create(&store, 2), &append(vec![file("a", 1, 10)]), &mut Batch("b1"));
        let extra = t.metadata.summary()["extra"].clone();
        assert!(block_on(store.store.exists(&extra)).unwrap());
        let (t, again) = run(&store, t, &append(vec![file("a", 1, 10)]), &mut Batch("b1"));
        assert!(again.skipped);
        assert_eq!((again.version, again.snapshot_id), (info.version, info.snapshot_id));
        assert_eq!(t.version, 2);
    }

    #[test]
    fn many_manifests_merge() {
        let store = Mem::at_ms(1_000);
        let mut t = create(&store, 2);
        for i in 0..9 {
            t = run(&store, t, &append(vec![file(&format!("f{i}"), i, 1)]), &mut NoHooks).0;
        }
        assert_eq!(block_on(t.manifests(&store.store)).unwrap().len(), 2, "eight merged into one, plus the new one");
        assert_eq!(live(&store).len(), 9);
    }

    #[test]
    fn a_clock_behind_the_table_keeps_the_logs_in_order() {
        let store = Mem::at_ms(5_000);
        let (t, _) = run(&store, create(&store, 2), &append(vec![file("a", 1, 10)]), &mut NoHooks);
        let late = Mem::at_ms(1_000);
        for uri in store.store.uris() {
            block_on(late.store.put(&uri, block_on(store.store.get(&uri)).unwrap().unwrap())).unwrap();
        }
        let (t, _) = run(&late, t, &append(vec![file("b", 1, 10)]), &mut NoHooks);
        assert_eq!((t.metadata.current_snapshot().unwrap().timestamp_ms, t.metadata.last_updated_ms()), (5_000, 5_000));
    }

    #[test]
    fn a_replace_needs_a_sequence_number_per_added_file() {
        let store = Mem::at_ms(1_000);
        let (t, _) = run(&store, create(&store, 2), &append(vec![file("a", 1, 10)]), &mut NoHooks);
        let removed = files_named(&t, "a.parquet");
        let change =
            Change::Replace { removed, added: vec![file("b", 1, 5), file("c", 1, 5)], sequence_numbers: vec![1] };
        let err = try_run(&store, t, &change, &mut NoHooks);
        assert!(matches!(err, Err(Error::Invalid(_))), "{err:?}");
    }

    #[test]
    fn only_v2_tables_are_written() {
        let store = Mem::at_ms(1_000);
        let err = try_run(&store, create(&store, 1), &append(vec![]), &mut NoHooks);
        assert!(matches!(err, Err(Error::Unsupported(_))));
    }
}
