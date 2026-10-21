//! Table metadata (`*.metadata.json`), kept as JSON so fields this crate doesn't model survive a
//! rewrite, with typed views for what readers and writers need. Reading covers format versions 1
//! to 3; updates (feature `write`) are for format v2, made the way iceberg-rust's metadata builder
//! makes them.
//! <https://iceberg.apache.org/spec/#table-metadata>

use std::collections::HashMap;

use serde_json::Value;

use crate::partition::{PartitionFieldType, PartitionSpec};
use crate::types::Schema;
use crate::{Error, Result, corrupt};

/// The format version this crate writes.
pub const WRITABLE_FORMAT_VERSION: u8 = 2;

/// A table's metadata at one version.
#[derive(Clone, Debug, PartialEq)]
pub struct Metadata(Value);

/// A snapshot: the table's state after one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Its id.
    pub snapshot_id: i64,
    /// The snapshot it was committed on.
    pub parent_snapshot_id: Option<i64>,
    /// Its sequence number (0 in format v1).
    pub sequence_number: i64,
    /// When it was made (ms since the epoch).
    pub timestamp_ms: i64,
    /// Its manifest list (format v1 snapshots may list manifests inline instead).
    pub manifest_list: Option<String>,
    /// Its summary, including `operation`.
    pub summary: HashMap<String, String>,
    /// The schema it was written with.
    pub schema_id: Option<i32>,
}

impl Snapshot {
    fn from_json(s: &Value) -> Option<Self> {
        Some(Self {
            snapshot_id: s.get("snapshot-id")?.as_i64()?,
            parent_snapshot_id: s.get("parent-snapshot-id").and_then(Value::as_i64),
            sequence_number: s.get("sequence-number").and_then(Value::as_i64).unwrap_or(0),
            timestamp_ms: s.get("timestamp-ms").and_then(Value::as_i64).unwrap_or(0),
            manifest_list: s.get("manifest-list").and_then(Value::as_str).map(str::to_string),
            summary: strings(s.get("summary")),
            schema_id: s.get("schema-id").and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok()),
        })
    }

    /// `append`, `replace`, `overwrite` or `delete`.
    pub fn operation(&self) -> Option<&str> {
        self.summary.get("operation").map(String::as_str)
    }
}

fn strings(v: Option<&Value>) -> HashMap<String, String> {
    let Some(Value::Object(m)) = v else { return HashMap::new() };
    m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()
}

fn find_by<'a>(list: Option<&'a Value>, key: &str, id: i64) -> Option<&'a Value> {
    list?.as_array()?.iter().find(|v| v.get(key).and_then(Value::as_i64) == Some(id))
}

impl Metadata {
    /// Parses table metadata of format version 1, 2 or 3.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let value: Value = serde_json::from_slice(bytes).map_err(|e| corrupt(format!("table metadata: {e}")))?;
        Self::from_json(value)
    }

    /// Table metadata from parsed JSON.
    pub fn from_json(value: Value) -> Result<Self> {
        match value.get("format-version").and_then(Value::as_u64) {
            Some(1..=3) => Ok(Self(value)),
            Some(v) => Err(Error::Unsupported(format!("Iceberg format version {v}"))),
            None => Err(corrupt("table metadata without a format version")),
        }
    }

    /// The format version (1 to 3).
    pub fn format_version(&self) -> u8 {
        self.0.get("format-version").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok()).unwrap_or(0)
    }

    /// An error unless this crate can write the table (format v2).
    pub fn check_writable(&self) -> Result<()> {
        match self.format_version() {
            WRITABLE_FORMAT_VERSION => Ok(()),
            v => Err(Error::Unsupported(format!(
                "writing Iceberg format version {v} tables (only version {WRITABLE_FORMAT_VERSION})"
            ))),
        }
    }

    /// The metadata as JSON.
    pub fn as_json(&self) -> &Value {
        &self.0
    }

    /// The metadata as JSON text.
    pub fn to_json(&self) -> String {
        self.0.to_string()
    }

    /// The table's root location.
    pub fn location(&self) -> &str {
        self.0.get("location").and_then(Value::as_str).unwrap_or_default()
    }

    /// The table's UUID.
    pub fn table_uuid(&self) -> Option<&str> {
        self.0.get("table-uuid").and_then(Value::as_str)
    }

    /// The table properties.
    pub fn properties(&self) -> HashMap<String, String> {
        strings(self.0.get("properties"))
    }

    /// When the metadata was last updated (ms since the epoch).
    pub fn last_updated_ms(&self) -> i64 {
        self.0.get("last-updated-ms").and_then(Value::as_i64).unwrap_or(0)
    }

    /// The current snapshot's id.
    pub fn current_snapshot_id(&self) -> Option<i64> {
        self.0.get("current-snapshot-id").and_then(Value::as_i64).filter(|&id| id >= 0)
    }

    /// Every snapshot, in the order the metadata lists them.
    pub fn snapshots(&self) -> Vec<Snapshot> {
        let Some(Value::Array(list)) = self.0.get("snapshots") else { return vec![] };
        list.iter().filter_map(Snapshot::from_json).collect()
    }

    /// The snapshot with id `id`.
    pub fn snapshot(&self, id: i64) -> Option<Snapshot> {
        Snapshot::from_json(find_by(self.0.get("snapshots"), "snapshot-id", id)?)
    }

    /// Whether a snapshot with id `id` exists.
    pub fn has_snapshot(&self, id: i64) -> bool {
        find_by(self.0.get("snapshots"), "snapshot-id", id).is_some()
    }

    /// The current snapshot.
    pub fn current_snapshot(&self) -> Option<Snapshot> {
        self.snapshot(self.current_snapshot_id()?)
    }

    /// The current snapshot and its ancestors, newest first, as far as they are still listed.
    pub fn ancestors(&self) -> Vec<Snapshot> {
        let mut out: Vec<Snapshot> = vec![];
        let mut next = self.current_snapshot_id();
        while let Some(s) = next.and_then(|id| self.snapshot(id)) {
            // A cycle would be corrupt metadata; stop rather than loop.
            if out.iter().any(|a| a.snapshot_id == s.snapshot_id) {
                break;
            }
            next = s.parent_snapshot_id;
            out.push(s);
        }
        out
    }

    /// The current snapshot's summary.
    pub fn summary(&self) -> HashMap<String, String> {
        self.current_snapshot().map(|s| s.summary).unwrap_or_default()
    }

    /// The current snapshot's manifest list.
    pub fn manifest_list(&self) -> Option<String> {
        self.current_snapshot()?.manifest_list
    }

    /// The sequence number the next snapshot gets.
    pub fn next_sequence_number(&self) -> i64 {
        self.0.get("last-sequence-number").and_then(Value::as_i64).unwrap_or(0).saturating_add(1)
    }

    /// The current schema's id.
    pub fn schema_id(&self) -> i32 {
        self.0.get("current-schema-id").and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok()).unwrap_or(0)
    }

    /// The current schema as Iceberg JSON (`schema` in format v1 metadata without `schemas`).
    pub fn schema_json(&self) -> Result<&Value> {
        find_by(self.0.get("schemas"), "schema-id", self.schema_id().into())
            .or_else(|| self.0.get("schema"))
            .ok_or_else(|| corrupt("table metadata without a current schema"))
    }

    /// The current schema.
    pub fn schema(&self) -> Result<Schema> {
        Schema::from_json(self.schema_json()?)
    }

    /// The default partition spec's id and fields as Iceberg JSON (`partition-spec` in format v1
    /// metadata without `partition-specs`).
    pub fn partition_spec_json(&self) -> Result<(i32, &Value)> {
        let id = self.0.get("default-spec-id").and_then(Value::as_i64).unwrap_or(0);
        let fields = match find_by(self.0.get("partition-specs"), "spec-id", id) {
            Some(spec) => spec.get("fields").ok_or_else(|| corrupt("partition spec without fields"))?,
            None => self.0.get("partition-spec").ok_or_else(|| corrupt("table metadata without a partition spec"))?,
        };
        Ok((i32::try_from(id).map_err(|_| corrupt("partition spec id"))?, fields))
    }

    /// The default partition spec.
    pub fn partition_spec(&self) -> Result<PartitionSpec> {
        let (id, fields) = self.partition_spec_json()?;
        PartitionSpec::from_fields(id, fields)
    }

    /// The default partition spec's fields with the types of their values.
    pub fn partition_type(&self) -> Result<Vec<PartitionFieldType>> {
        self.partition_spec()?.partition_type(&self.schema()?)
    }

    /// The default sort order's id.
    pub fn sort_order_id(&self) -> i32 {
        self.0.get("default-sort-order-id").and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok()).unwrap_or(0)
    }

    /// Entries in the metadata log.
    pub fn metadata_log_len(&self) -> usize {
        self.0.get("metadata-log").and_then(Value::as_array).map_or(0, Vec::len)
    }

    /// The current schema as Arrow, fields carrying their Iceberg field ids.
    #[cfg(feature = "arrow")]
    pub fn arrow_schema(&self) -> Result<arrow_schema::SchemaRef> {
        Ok(std::sync::Arc::new(crate::arrow::schema_to_arrow(&self.schema()?)?))
    }
}

#[cfg(feature = "write")]
pub use write::NewSnapshot;

#[cfg(feature = "write")]
mod write {
    use std::collections::HashMap;

    use serde_json::{Map, Value, json};

    use super::Metadata;
    use crate::{Result, corrupt};

    /// How many earlier metadata files the metadata log keeps unless the table sets
    /// `write.metadata.previous-versions-max`.
    const PREVIOUS_VERSIONS_MAX: usize = 100;

    /// A snapshot to add (see [`Metadata::with_snapshot`]).
    #[derive(Clone, Debug)]
    pub struct NewSnapshot {
        /// Its id.
        pub snapshot_id: i64,
        /// Its sequence number.
        pub sequence_number: i64,
        /// When it was made (ms since the epoch).
        pub timestamp_ms: i64,
        /// Its manifest list.
        pub manifest_list: String,
        /// `append`, `replace`, `overwrite` or `delete`.
        pub operation: String,
        /// Summary properties besides the operation.
        pub summary: HashMap<String, String>,
    }

    impl NewSnapshot {
        fn summary_json(&self) -> Value {
            let mut s: Map<String, Value> = self.summary.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            s.insert("operation".into(), json!(self.operation));
            Value::Object(s)
        }
    }

    fn push(obj: &mut Map<String, Value>, key: &str, value: Value) {
        match obj.get_mut(key) {
            Some(Value::Array(a)) => a.push(value),
            _ => {
                obj.insert(key.into(), Value::Array(vec![value]));
            }
        }
    }

    impl Metadata {
        fn object(&mut self) -> Result<&mut Map<String, Value>> {
            self.0.as_object_mut().ok_or_else(|| corrupt("table metadata isn't an object"))
        }

        /// Records `previous` (the metadata file this version replaces) in the metadata log,
        /// trimmed to the table's `write.metadata.previous-versions-max`.
        fn log_previous(&mut self, previous: &str, previous_updated_ms: i64) -> Result<()> {
            let keep = self
                .properties()
                .get("write.metadata.previous-versions-max")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(PREVIOUS_VERSIONS_MAX)
                .max(1);
            let obj = self.object()?;
            push(obj, "metadata-log", json!({"metadata-file": previous, "timestamp-ms": previous_updated_ms}));
            if let Some(Value::Array(log)) = obj.get_mut("metadata-log")
                && log.len() > keep
            {
                log.drain(..log.len() - keep);
            }
            Ok(())
        }

        /// This metadata with `snapshot` added as the current snapshot of `main`, `previous` (this
        /// version's file) recorded in the metadata log, updated at `now_ms`.
        pub fn with_snapshot(&self, snapshot: &NewSnapshot, previous: &str, now_ms: i64) -> Result<Self> {
            self.check_writable()?;
            let mut m = self.clone();
            let mut s = json!({
                "snapshot-id": snapshot.snapshot_id,
                "sequence-number": snapshot.sequence_number,
                "timestamp-ms": snapshot.timestamp_ms,
                "manifest-list": snapshot.manifest_list,
                "summary": snapshot.summary_json(),
                "schema-id": self.schema_id(),
            });
            if let (Some(parent), Some(s)) = (self.current_snapshot_id(), s.as_object_mut()) {
                s.insert("parent-snapshot-id".into(), json!(parent));
            }
            m.log_previous(previous, self.last_updated_ms())?;
            let obj = m.object()?;
            push(obj, "snapshots", s);
            push(
                obj,
                "snapshot-log",
                json!({"snapshot-id": snapshot.snapshot_id, "timestamp-ms": snapshot.timestamp_ms}),
            );
            obj.insert("current-snapshot-id".into(), json!(snapshot.snapshot_id));
            obj.insert("last-sequence-number".into(), json!(snapshot.sequence_number));
            obj.insert("last-updated-ms".into(), json!(now_ms));
            let refs = obj.entry("refs").or_insert_with(|| Value::Object(Map::new()));
            if let Some(refs) = refs.as_object_mut() {
                refs.insert("main".into(), json!({"snapshot-id": snapshot.snapshot_id, "type": "branch"}));
            }
            Ok(m)
        }

        /// This metadata without the snapshots `ids`, as iceberg-rust's builder removes them: refs to
        /// them go, and the snapshot log keeps only what follows the last entry of a removed snapshot
        /// (so it has no gaps). `previous` is recorded in the metadata log. The current snapshot can't be
        /// removed: the table would have none and read as empty.
        pub fn without_snapshots(&self, ids: &[i64], previous: &str, now_ms: i64) -> Result<Self> {
            self.check_writable()?;
            if let Some(current) = self.current_snapshot_id().filter(|id| ids.contains(id)) {
                return Err(crate::Error::Invalid(format!("removing the current snapshot {current}")));
            }
            let mut m = self.clone();
            m.log_previous(previous, self.last_updated_ms())?;
            let obj = m.object()?;
            let removed = |v: &Value| v.get("snapshot-id").and_then(Value::as_i64).is_some_and(|id| ids.contains(&id));
            if let Some(Value::Array(snapshots)) = obj.get_mut("snapshots") {
                snapshots.retain(|s| !removed(s));
            }
            if let Some(Value::Object(refs)) = obj.get_mut("refs") {
                refs.retain(|_, r| !removed(r));
            }
            if let Some(Value::Array(log)) = obj.get_mut("snapshot-log") {
                let mut kept = vec![];
                for entry in log.drain(..) {
                    if removed(&entry) {
                        kept.clear();
                    } else {
                        kept.push(entry);
                    }
                }
                *log = kept;
            }
            obj.insert("last-updated-ms".into(), json!(now_ms));
            Ok(m)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::types::PrimitiveType;

    fn v2() -> Metadata {
        Metadata::from_json(json!({
            "format-version": 2, "table-uuid": "u", "location": "memory://t", "last-sequence-number": 2,
            "last-updated-ms": 10, "current-schema-id": 1,
            "schemas": [
                {"schema-id": 0, "type": "struct", "fields": []},
                {"schema-id": 1, "type": "struct", "fields": [{"id": 1, "name": "time", "required": true, "type": "timestamptz"}]}
            ],
            "default-spec-id": 0,
            "partition-specs": [{"spec-id": 0, "fields": [{"source-id": 1, "field-id": 1000, "name": "time_day", "transform": "day"}]}],
            "properties": {"write.metadata.previous-versions-max": "2", "n": 1},
            "current-snapshot-id": 2,
            "snapshots": [
                {"snapshot-id": 1, "sequence-number": 1, "timestamp-ms": 1, "manifest-list": "l1", "summary": {"operation": "append"}},
                {"snapshot-id": 2, "parent-snapshot-id": 1, "sequence-number": 2, "timestamp-ms": 2, "manifest-list": "l2", "summary": {"operation": "replace"}}
            ],
            "snapshot-log": [{"snapshot-id": 1, "timestamp-ms": 1}, {"snapshot-id": 2, "timestamp-ms": 2}],
            "metadata-log": [],
            "unknown-field": {"kept": true}
        }))
        .unwrap()
    }

    #[test]
    fn reads_v2() {
        let m = v2();
        assert_eq!((m.format_version(), m.location(), m.table_uuid()), (2, "memory://t", Some("u")));
        assert_eq!(m.properties().len(), 1, "non-string properties are skipped");
        assert_eq!(m.schema().unwrap().fields[0].name, "time");
        assert_eq!(m.partition_type().unwrap()[0].ty, PrimitiveType::Date);
        assert_eq!(m.current_snapshot().unwrap().operation(), Some("replace"));
        let ancestors: Vec<i64> = m.ancestors().iter().map(|s| s.snapshot_id).collect();
        assert_eq!(ancestors, [2, 1]);
        assert_eq!(m.manifest_list().as_deref(), Some("l2"));
        assert_eq!(m.next_sequence_number(), 3);
        assert!(m.check_writable().is_ok());
    }

    #[test]
    fn reads_v1() {
        let m = Metadata::from_json(json!({
            "format-version": 1, "location": "s3://b/t",
            "schema": {"type": "struct", "fields": [{"id": 1, "name": "x", "required": false, "type": "long"}]},
            "partition-spec": [{"source-id": 1, "name": "x", "transform": "identity"}],
            "snapshots": []
        }))
        .unwrap();
        assert_eq!(m.schema().unwrap().fields[0].name, "x");
        assert_eq!(m.partition_spec().unwrap().fields[0].field_id, 1000);
        assert_eq!(m.current_snapshot(), None);
        assert!(matches!(m.check_writable(), Err(Error::Unsupported(_))));
    }

    #[test]
    fn a_sequence_number_at_the_limit_saturates() {
        let m = Metadata::from_json(json!({"format-version": 2, "last-sequence-number": i64::MAX})).unwrap();
        assert_eq!(m.next_sequence_number(), i64::MAX);
    }

    #[test]
    fn rejects_unknown_versions() {
        assert!(matches!(Metadata::parse(br#"{"format-version": 4}"#), Err(Error::Unsupported(_))));
        assert!(matches!(Metadata::parse(br#"{"location": "x"}"#), Err(Error::Corrupt(_))));
        assert!(matches!(Metadata::parse(b"{"), Err(Error::Corrupt(_))));
    }

    #[cfg(feature = "write")]
    #[test]
    fn updates_keep_unknown_fields_and_trim_the_log() {
        let m = v2();
        let s = NewSnapshot {
            snapshot_id: 3,
            sequence_number: 3,
            timestamp_ms: 20,
            manifest_list: "l3".into(),
            operation: "append".into(),
            summary: HashMap::from([("k".to_string(), "v".to_string())]),
        };
        let m = m.with_snapshot(&s, "v1.metadata.json", 20).unwrap();
        let m = m.with_snapshot(&NewSnapshot { snapshot_id: 4, ..s.clone() }, "v2.metadata.json", 21).unwrap();
        let m = m.with_snapshot(&NewSnapshot { snapshot_id: 5, ..s }, "v3.metadata.json", 22).unwrap();
        assert_eq!(m.current_snapshot_id(), Some(5));
        assert_eq!(m.snapshot(3).unwrap().parent_snapshot_id, Some(2));
        assert_eq!(m.snapshot(3).unwrap().summary["k"], "v");
        assert_eq!(m.metadata_log_len(), 2, "previous-versions-max");
        assert_eq!(m.as_json()["unknown-field"]["kept"], true);
        assert_eq!(m.as_json()["refs"]["main"]["snapshot-id"], 5);

        let current = m.without_snapshots(&[1, 5], "v4.metadata.json", 23);
        assert!(matches!(current, Err(Error::Invalid(_))), "the current snapshot stays");
        let m = m.without_snapshots(&[1, 2], "v4.metadata.json", 23).unwrap();
        assert!(!m.has_snapshot(1) && !m.has_snapshot(2) && m.has_snapshot(3));
        let log: Vec<i64> = m.as_json()["snapshot-log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["snapshot-id"].as_i64().unwrap())
            .collect();
        assert_eq!(log, [3, 4, 5]);
    }
}
