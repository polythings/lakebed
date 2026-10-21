//! Manifests and manifest lists: the Avro files that list a snapshot's data and delete files.
//! Reading covers format versions 1 to 3; writing (feature `write`) makes format v2 data
//! manifests in the layout iceberg-rust writes: the same Avro schemas, field ids and header
//! metadata.
//! <https://iceberg.apache.org/spec/#manifests>

use std::collections::BTreeMap;
use std::sync::Arc;

use apache_avro::types::Value;
use serde_json::Value as Json;

use crate::avro::{Header, field, field_of, int, long, records, required, string};
use crate::datum::{Datum, from_be};
use crate::partition::{PartitionFieldType, PartitionSpec};
use crate::types::{PrimitiveType, Schema};
use crate::{Result, corrupt};

/// Whether a manifest entry adds, keeps or removes its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// Carried over from an earlier snapshot.
    Existing = 0,
    /// Added by the manifest's snapshot.
    Added = 1,
    /// Removed by the manifest's snapshot.
    Deleted = 2,
}

/// What a file holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Content {
    /// Rows.
    #[default]
    Data = 0,
    /// Deleted row positions (a Parquet delete file, or a Puffin deletion vector in format v3).
    PositionDeletes = 1,
    /// Deleted rows by the values of `equality_ids` columns.
    EqualityDeletes = 2,
}

/// What a manifest lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ManifestContent {
    /// Data files.
    #[default]
    Data = 0,
    /// Delete files.
    Deletes = 1,
}

/// A data or delete file as a manifest records it. Bounds are Iceberg's single-value binary
/// encoding ([`Datum::from_bytes`]), keyed by field id.
#[derive(Clone, Debug, PartialEq)]
pub struct DataFile {
    /// What the file holds.
    pub content: Content,
    /// Absolute URI.
    pub path: String,
    /// `PARQUET`, `AVRO`, `ORC` or `PUFFIN`.
    pub file_format: String,
    /// The partition tuple, in the order of the spec's fields (empty for unpartitioned files).
    pub partition: Vec<Option<Datum>>,
    /// Rows.
    pub record_count: u64,
    /// Bytes.
    pub file_size: u64,
    /// Compressed bytes per field id.
    pub column_sizes: BTreeMap<i32, u64>,
    /// Values per field id.
    pub value_counts: BTreeMap<i32, u64>,
    /// Nulls per field id.
    pub null_counts: BTreeMap<i32, u64>,
    /// NaNs per field id.
    pub nan_counts: BTreeMap<i32, u64>,
    /// Smallest value per field id.
    pub lower_bounds: BTreeMap<i32, Vec<u8>>,
    /// Largest value per field id.
    pub upper_bounds: BTreeMap<i32, Vec<u8>>,
    /// Row group offsets.
    pub split_offsets: Option<Vec<i64>>,
    /// For equality deletes: the columns that identify deleted rows.
    pub equality_ids: Option<Vec<i32>>,
    /// The table sort order the file follows.
    pub sort_order_id: Option<i32>,
    /// The row id of the file's first row (format v3).
    pub first_row_id: Option<i64>,
    /// For position deletes and deletion vectors: the one data file they apply to.
    pub referenced_data_file: Option<String>,
    /// For deletion vectors: the vector's offset in the Puffin file.
    pub content_offset: Option<i64>,
    /// For deletion vectors: the vector's length.
    pub content_size: Option<i64>,
}

impl Default for DataFile {
    fn default() -> Self {
        Self {
            content: Content::Data,
            path: String::new(),
            file_format: "PARQUET".into(),
            partition: vec![],
            record_count: 0,
            file_size: 0,
            column_sizes: BTreeMap::new(),
            value_counts: BTreeMap::new(),
            null_counts: BTreeMap::new(),
            nan_counts: BTreeMap::new(),
            lower_bounds: BTreeMap::new(),
            upper_bounds: BTreeMap::new(),
            split_offsets: None,
            equality_ids: None,
            sort_order_id: None,
            first_row_id: None,
            referenced_data_file: None,
            content_offset: None,
            content_size: None,
        }
    }
}

/// A file in a snapshot, with the sequence numbers that order its rows against other files'.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Added, existing or deleted.
    pub status: Status,
    /// The snapshot that added (or deleted) the file.
    pub snapshot_id: i64,
    /// Data sequence number: orders the file's rows against other files' rows and deletes.
    pub sequence_number: i64,
    /// Sequence number of the commit that added the file.
    pub file_sequence_number: i64,
    /// The file.
    pub file: Arc<DataFile>,
}

/// An entry of a manifest list. The default (sequence numbers and snapshot 0) suits reading a
/// manifest's files without the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManifestFile {
    /// Absolute URI of the manifest.
    pub path: String,
    /// Its size in bytes.
    pub length: i64,
    /// The partition spec its files use.
    pub partition_spec_id: i32,
    /// Data or delete files.
    pub content: ManifestContent,
    /// Sequence number of the snapshot that wrote it.
    pub sequence_number: i64,
    /// Smallest data sequence number of its files.
    pub min_sequence_number: i64,
    /// The snapshot that wrote it.
    pub added_snapshot_id: i64,
    /// Files added.
    pub added_files: i32,
    /// Files carried over.
    pub existing_files: i32,
    /// Files removed.
    pub deleted_files: i32,
    /// Rows in added files.
    pub added_rows: i64,
    /// Rows in existing files.
    pub existing_rows: i64,
    /// Rows in deleted files.
    pub deleted_rows: i64,
    /// The range of each partition field's values (`None` for unpartitioned manifests).
    pub partitions: Option<Vec<FieldSummary>>,
    /// The row id of the manifest's first row (format v3).
    pub first_row_id: Option<i64>,
}

/// The range of one partition field's values in a manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSummary {
    /// Some file has no value for the field.
    pub contains_null: bool,
    /// Some file's value is NaN (`None`: not recorded).
    pub contains_nan: Option<bool>,
    /// Lowest value, single-value serialized.
    pub lower: Option<Vec<u8>>,
    /// Highest value, single-value serialized.
    pub upper: Option<Vec<u8>>,
}

/// A sequence number the manifest list assigns when the snapshot commits.
pub const UNASSIGNED: i64 = -1;

// --- reading -------------------------------------------------------------------------------------

fn u64_of(v: Option<i64>) -> u64 {
    v.and_then(|v| u64::try_from(v).ok()).unwrap_or(0)
}

/// The manifests of a manifest list (format v1 to v3).
pub fn read_manifest_list(bytes: &[u8]) -> Result<Vec<ManifestFile>> {
    let (records, _) = records(bytes)?;
    records
        .iter()
        .map(|r| {
            let partitions = match field(r, "partitions") {
                Some(Value::Array(items)) => Some(items.iter().map(field_summary).collect()),
                _ => None,
            };
            let count = |names: &[&str]| int(field_of(r, names)).unwrap_or(0);
            let rows = |name: &str| long(field(r, name)).unwrap_or(0);
            Ok(ManifestFile {
                path: string(field(r, "manifest_path"))
                    .ok_or_else(|| corrupt("manifest list entry without a path"))?
                    .to_string(),
                length: required(long(field(r, "manifest_length")), "length")?,
                partition_spec_id: int(field(r, "partition_spec_id")).unwrap_or(0),
                content: match long(field(r, "content")) {
                    None | Some(0) => ManifestContent::Data,
                    Some(1) => ManifestContent::Deletes,
                    Some(other) => return Err(corrupt(format!("manifest content {other}"))),
                },
                sequence_number: long(field(r, "sequence_number")).unwrap_or(0),
                min_sequence_number: long(field(r, "min_sequence_number")).unwrap_or(0),
                added_snapshot_id: long(field(r, "added_snapshot_id")).unwrap_or(0),
                // Format v1 names the counts `*_data_files_count`.
                added_files: count(&["added_files_count", "added_data_files_count"]),
                existing_files: count(&["existing_files_count", "existing_data_files_count"]),
                deleted_files: count(&["deleted_files_count", "deleted_data_files_count"]),
                added_rows: rows("added_rows_count"),
                existing_rows: rows("existing_rows_count"),
                deleted_rows: rows("deleted_rows_count"),
                partitions,
                first_row_id: long(field(r, "first_row_id")),
            })
        })
        .collect()
}

fn field_summary(p: &Value) -> FieldSummary {
    let bytes = |name: &str| match field(p, name) {
        Some(Value::Bytes(b)) => Some(b.clone()),
        _ => None,
    };
    FieldSummary {
        contains_null: matches!(field(p, "contains_null"), Some(Value::Boolean(true))),
        contains_nan: match field(p, "contains_nan") {
            Some(Value::Boolean(b)) => Some(*b),
            _ => None,
        },
        lower: bytes("lower_bound"),
        upper: bytes("upper_bound"),
    }
}

fn long_map(v: Option<&Value>) -> BTreeMap<i32, u64> {
    let Some(Value::Array(items)) = v else { return BTreeMap::new() };
    items
        .iter()
        .filter_map(|kv| Some((int(field(kv, "key"))?, u64::try_from(long(field(kv, "value"))?).ok()?)))
        .collect()
}

fn bytes_map(v: Option<&Value>) -> BTreeMap<i32, Vec<u8>> {
    let Some(Value::Array(items)) = v else { return BTreeMap::new() };
    items
        .iter()
        .filter_map(|kv| match field(kv, "value") {
            Some(Value::Bytes(b)) => Some((int(field(kv, "key"))?, b.clone())),
            _ => None,
        })
        .collect()
}

fn longs(v: Option<&Value>) -> Option<Vec<i64>> {
    match v {
        Some(Value::Array(items)) => Some(items.iter().filter_map(|x| long(Some(x))).collect()),
        _ => None,
    }
}

/// The partition field types a manifest's header records (its `schema` and `partition-spec`), if
/// it records them and they parse.
fn header_partition_type(metadata: &Header) -> Option<Vec<PartitionFieldType>> {
    let json = |key: &str| serde_json::from_slice::<Json>(metadata.get(key)?).ok();
    let schema = Schema::from_json(&json("schema")?).ok()?;
    let spec_id = std::str::from_utf8(metadata.get("partition-spec-id")?).ok()?.parse().ok()?;
    let spec = PartitionSpec::from_fields(spec_id, &json("partition-spec")?).ok()?;
    spec.partition_type(&schema).ok()
}

/// A partition value. `ty` is the field's type when the manifest header gives it; without it the
/// Avro type decides.
fn partition_datum(v: &Value, ty: Option<&PrimitiveType>) -> Result<Option<Datum>> {
    use PrimitiveType as P;
    Ok(Some(match (v, ty) {
        (Value::Null, _) => return Ok(None),
        (Value::Boolean(b), _) => Datum::Boolean(*b),
        (Value::Int(n) | Value::Date(n), Some(P::Date)) | (Value::Date(n), None) => Datum::Date(*n),
        (Value::Int(n), _) => Datum::Int(*n),
        (Value::Long(n) | Value::TimeMicros(n), Some(P::Time)) | (Value::TimeMicros(n), None) => Datum::Time(*n),
        (Value::Long(n) | Value::TimestampMicros(n) | Value::LocalTimestampMicros(n), Some(P::Timestamp))
        | (Value::LocalTimestampMicros(n), None) => Datum::Timestamp(*n),
        (Value::Long(n) | Value::TimestampMicros(n), Some(P::Timestamptz)) | (Value::TimestampMicros(n), None) => {
            Datum::Timestamptz(*n)
        }
        (Value::Long(n) | Value::TimestampNanos(n) | Value::LocalTimestampNanos(n), Some(P::TimestampNs))
        | (Value::LocalTimestampNanos(n), None) => Datum::TimestampNs(*n),
        (Value::Long(n) | Value::TimestampNanos(n), Some(P::TimestamptzNs)) | (Value::TimestampNanos(n), None) => {
            Datum::TimestamptzNs(*n)
        }
        (Value::Long(n), _) => Datum::Long(*n),
        (Value::Float(x), _) => Datum::Float(*x),
        (Value::Double(x), _) => Datum::Double(*x),
        (Value::String(s), _) => Datum::String(s.clone()),
        (Value::Bytes(b) | Value::Fixed(_, b), Some(P::Decimal { precision, scale })) if b.len() <= 16 => {
            Datum::Decimal { unscaled: from_be(b), precision: *precision, scale: *scale }
        }
        (Value::Decimal(d), Some(P::Decimal { precision, scale })) => {
            let b = Vec::<u8>::try_from(d).map_err(crate::avro::error)?;
            if b.len() > 16 {
                return Err(corrupt("a decimal partition value of more than 16 bytes"));
            }
            Datum::Decimal { unscaled: from_be(&b), precision: *precision, scale: *scale }
        }
        (Value::Uuid(u), _) => Datum::Uuid(*u.as_bytes()),
        (Value::Fixed(16, b), Some(P::Uuid)) => Datum::Uuid(b.as_slice().try_into().map_err(crate::avro::error)?),
        (Value::Fixed(_, b), _) => Datum::Fixed(b.clone()),
        (Value::Bytes(b), _) => Datum::Binary(b.clone()),
        (other, _) => return Err(corrupt(format!("partition value {other:?}"))),
    }))
}

/// The entries of `manifest` (format v1 to v3), with the snapshot ids and sequence numbers they
/// inherit from it filled in.
pub fn read_manifest(manifest: &ManifestFile, bytes: &[u8]) -> Result<Vec<Entry>> {
    let (records, metadata) = records(bytes)?;
    let types = header_partition_type(&metadata);
    records
        .iter()
        .map(|r| {
            let status = match int(field(r, "status")) {
                Some(0) => Status::Existing,
                Some(1) => Status::Added,
                Some(2) => Status::Deleted,
                other => return Err(corrupt(format!("manifest entry status {other:?}"))),
            };
            let df = required(field(r, "data_file"), "data_file")?;
            let partition = match field(df, "partition") {
                Some(Value::Record(fields)) => fields
                    .iter()
                    .enumerate()
                    .map(|(i, (_, v))| {
                        let ty = types.as_ref().and_then(|t| t.get(i)).map(|t| &t.ty);
                        partition_datum(crate::avro::strip_union(v), ty)
                    })
                    .collect::<Result<_>>()?,
                _ => vec![],
            };
            let file = DataFile {
                content: match long(field(df, "content")) {
                    None | Some(0) => Content::Data,
                    Some(1) => Content::PositionDeletes,
                    Some(2) => Content::EqualityDeletes,
                    Some(other) => return Err(corrupt(format!("data file content {other}"))),
                },
                path: string(field(df, "file_path")).ok_or_else(|| corrupt("data file without a path"))?.to_string(),
                file_format: string(field(df, "file_format")).unwrap_or("PARQUET").to_string(),
                partition,
                record_count: u64_of(Some(required(long(field(df, "record_count")), "record_count")?)),
                file_size: u64_of(Some(required(long(field(df, "file_size_in_bytes")), "file size")?)),
                column_sizes: long_map(field(df, "column_sizes")),
                value_counts: long_map(field(df, "value_counts")),
                null_counts: long_map(field(df, "null_value_counts")),
                nan_counts: long_map(field(df, "nan_value_counts")),
                lower_bounds: bytes_map(field(df, "lower_bounds")),
                upper_bounds: bytes_map(field(df, "upper_bounds")),
                split_offsets: longs(field(df, "split_offsets")),
                equality_ids: longs(field(df, "equality_ids"))
                    .map(|ids| ids.into_iter().filter_map(|id| i32::try_from(id).ok()).collect()),
                sort_order_id: int(field(df, "sort_order_id")),
                first_row_id: long(field(df, "first_row_id")),
                referenced_data_file: string(field(df, "referenced_data_file")).map(str::to_string),
                content_offset: long(field(df, "content_offset")),
                content_size: long(field(df, "content_size_in_bytes")),
            };
            // Entries without sequence numbers inherit the manifest's (Iceberg's rule; format v1
            // manifests have none and the list gives 0).
            let inherit = |v: Option<i64>| v.unwrap_or(manifest.sequence_number);
            Ok(Entry {
                status,
                snapshot_id: long(field(r, "snapshot_id")).unwrap_or(manifest.added_snapshot_id),
                sequence_number: inherit(long(field(r, "sequence_number"))),
                file_sequence_number: inherit(long(field(r, "file_sequence_number"))),
                file: Arc::new(file),
            })
        })
        .collect()
}

// --- writing -------------------------------------------------------------------------------------

#[cfg(feature = "write")]
pub use write::{ManifestContext, ManifestWrite, manifest_file, write_manifest, write_manifest_list};

#[cfg(feature = "write")]
mod write {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::sync::Arc;

    use apache_avro::types::Value;
    use apache_avro::{Codec, Schema as AvroSchema, Writer};

    use super::{Content, DataFile, FieldSummary, ManifestContent, ManifestFile, Status, UNASSIGNED};
    use crate::avro::{error, null, some};
    use crate::datum::Datum;
    use crate::partition::PartitionFieldType;
    use crate::types::PrimitiveType;
    use crate::{Error, Result};

    /// What a manifest's header records about the table.
    #[derive(Clone, Debug)]
    pub struct ManifestContext<'a> {
        /// The current schema as Iceberg JSON.
        pub schema_json: &'a str,
        /// Its id.
        pub schema_id: i32,
        /// The partition spec's fields as Iceberg JSON (`[{"source-id":…,"field-id":…,…}]`).
        pub spec_fields_json: &'a str,
        /// The spec's id.
        pub spec_id: i32,
        /// The spec's fields with the types of their values ([`crate::PartitionSpec::partition_type`]).
        pub partition_type: &'a [PartitionFieldType],
    }

    /// An entry to write into a manifest: `None` fields are inherited from the manifest list.
    #[derive(Clone, Debug)]
    pub struct ManifestWrite {
        /// Added, existing or deleted.
        pub status: Status,
        /// The snapshot, or `None` to inherit the manifest's.
        pub snapshot_id: Option<i64>,
        /// Data sequence number, or `None` to inherit.
        pub sequence_number: Option<i64>,
        /// File sequence number, or `None` to inherit.
        pub file_sequence_number: Option<i64>,
        /// The file.
        pub file: Arc<DataFile>,
    }

    /// A name Avro accepts: Iceberg's convention replaces a leading digit with `_` and the digit, and
    /// other characters Avro doesn't allow with `_x` and their code in hex.
    fn avro_name(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        for (i, c) in name.chars().enumerate() {
            match c {
                'a'..='z' | 'A'..='Z' | '_' => out.push(c),
                '0'..='9' if i > 0 => out.push(c),
                '0'..='9' => {
                    out.push('_');
                    out.push(c);
                }
                _ => {
                    let _ = write!(out, "_x{:X}", u32::from(c));
                }
            }
        }
        out
    }

    /// The Avro type of partition values of type `ty`.
    fn avro_type(ty: &PrimitiveType) -> Result<String> {
        let logical = |base: &str, logical: &str, utc: Option<bool>| {
            let utc = utc.map_or_else(String::new, |u| format!(r#", "adjust-to-utc": {u}"#));
            format!(r#"{{"type": "{base}", "logicalType": "{logical}"{utc}}}"#)
        };
        Ok(match ty {
            PrimitiveType::Boolean => r#""boolean""#.into(),
            PrimitiveType::Int => r#""int""#.into(),
            PrimitiveType::Long => r#""long""#.into(),
            PrimitiveType::Float => r#""float""#.into(),
            PrimitiveType::Double => r#""double""#.into(),
            PrimitiveType::String => r#""string""#.into(),
            PrimitiveType::Binary => r#""bytes""#.into(),
            PrimitiveType::Date => logical("int", "date", None),
            PrimitiveType::Time => logical("long", "time-micros", None),
            PrimitiveType::Timestamp => logical("long", "timestamp-micros", Some(false)),
            PrimitiveType::Timestamptz => logical("long", "timestamp-micros", Some(true)),
            PrimitiveType::TimestampNs => logical("long", "timestamp-nanos", Some(false)),
            PrimitiveType::TimestamptzNs => logical("long", "timestamp-nanos", Some(true)),
            PrimitiveType::Decimal { .. } | PrimitiveType::Uuid | PrimitiveType::Fixed(_) => {
                return Err(Error::Unsupported(format!("writing {ty} partition values")));
            }
        })
    }

    /// A partition value as Avro, checked against the field's type.
    fn avro_value(d: &Datum, ty: &PrimitiveType) -> Result<Value> {
        if d.primitive_type() != *ty {
            return Err(Error::Invalid(format!("a {} partition value for a {ty} field", d.primitive_type())));
        }
        Ok(match d {
            Datum::Boolean(b) => Value::Boolean(*b),
            Datum::Int(v) => Value::Int(*v),
            Datum::Date(v) => Value::Date(*v),
            Datum::Long(v) => Value::Long(*v),
            Datum::Time(v) => Value::TimeMicros(*v),
            Datum::Timestamp(v) | Datum::Timestamptz(v) => Value::TimestampMicros(*v),
            Datum::TimestampNs(v) | Datum::TimestamptzNs(v) => Value::TimestampNanos(*v),
            Datum::Float(v) => Value::Float(*v),
            Datum::Double(v) => Value::Double(*v),
            Datum::String(s) => Value::String(s.clone()),
            Datum::Binary(b) => Value::Bytes(b.clone()),
            Datum::Decimal { .. } | Datum::Uuid(_) | Datum::Fixed(_) => {
                return Err(Error::Unsupported(format!("writing {ty} partition values")));
            }
        })
    }

    /// The Avro schema of manifest entries for a table partitioned as `partition`.
    fn manifest_schema(partition: &[PartitionFieldType]) -> Result<AvroSchema> {
        let partition_fields = partition
            .iter()
            .map(|p| {
                Ok(format!(
                    r#"{{"name": "{}", "type": ["null", {}], "default": null, "field-id": {}}}"#,
                    avro_name(&p.name),
                    avro_type(&p.ty)?,
                    p.field_id
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let map = |name: &str, key: i32, value: i32, ty: &str| {
            format!(
                r#"{{"name": "{name}", "type": ["null", {{"type": "array", "items": {{"type": "record", "name": "k{key}_v{value}", "fields": [{{"name": "key", "type": "int", "field-id": {key}}}, {{"name": "value", "type": "{ty}", "field-id": {value}}}]}}, "logicalType": "map"}}], "default": null"#
            )
        };
        let json = format!(
            r#"{{"type": "record", "name": "manifest_entry", "fields": [
                {{"name": "status", "type": "int", "field-id": 0}},
                {{"name": "snapshot_id", "type": ["null", "long"], "default": null, "field-id": 1}},
                {{"name": "sequence_number", "type": ["null", "long"], "default": null, "field-id": 3}},
                {{"name": "file_sequence_number", "type": ["null", "long"], "default": null, "field-id": 4}},
                {{"name": "data_file", "type": {{"type": "record", "name": "r2", "fields": [
                    {{"name": "content", "type": "int", "default": 0, "field-id": 134}},
                    {{"name": "file_path", "type": "string", "field-id": 100}},
                    {{"name": "file_format", "type": "string", "field-id": 101}},
                    {{"name": "partition", "type": {{"type": "record", "name": "r102", "fields": [{partition_fields}]}}, "field-id": 102}},
                    {{"name": "record_count", "type": "long", "field-id": 103}},
                    {{"name": "file_size_in_bytes", "type": "long", "field-id": 104}},
                    {}, "field-id": 108}},
                    {}, "field-id": 109}},
                    {}, "field-id": 110}},
                    {}, "field-id": 137}},
                    {}, "field-id": 125}},
                    {}, "field-id": 128}},
                    {{"name": "key_metadata", "type": ["null", "bytes"], "default": null, "field-id": 131}},
                    {{"name": "split_offsets", "type": ["null", {{"type": "array", "items": "long", "element-id": 133}}], "default": null, "field-id": 132}},
                    {{"name": "equality_ids", "type": ["null", {{"type": "array", "items": "int", "element-id": 136}}], "default": null, "field-id": 135}},
                    {{"name": "sort_order_id", "type": ["null", "int"], "default": null, "field-id": 140}},
                    {{"name": "first_row_id", "type": ["null", "long"], "default": null, "field-id": 142}},
                    {{"name": "referenced_data_file", "type": ["null", "string"], "default": null, "field-id": 143}},
                    {{"name": "content_offset", "type": ["null", "long"], "default": null, "field-id": 144}},
                    {{"name": "content_size_in_bytes", "type": ["null", "long"], "default": null, "field-id": 145}}
                ]}}, "field-id": 2}}
            ]}}"#,
            map("column_sizes", 117, 118, "long"),
            map("value_counts", 119, 120, "long"),
            map("null_value_counts", 121, 122, "long"),
            map("nan_value_counts", 138, 139, "long"),
            map("lower_bounds", 126, 127, "bytes"),
            map("upper_bounds", 129, 130, "bytes"),
        );
        AvroSchema::parse_str(&json).map_err(error)
    }

    const MANIFEST_LIST_SCHEMA: &str = r#"{"type": "record", "name": "manifest_file", "fields": [
        {"name": "manifest_path", "type": "string", "field-id": 500},
        {"name": "manifest_length", "type": "long", "field-id": 501},
        {"name": "partition_spec_id", "type": "int", "field-id": 502},
        {"name": "content", "type": "int", "field-id": 517},
        {"name": "sequence_number", "type": "long", "field-id": 515},
        {"name": "min_sequence_number", "type": "long", "field-id": 516},
        {"name": "added_snapshot_id", "type": "long", "field-id": 503},
        {"name": "added_files_count", "type": "int", "field-id": 504},
        {"name": "existing_files_count", "type": "int", "field-id": 505},
        {"name": "deleted_files_count", "type": "int", "field-id": 506},
        {"name": "added_rows_count", "type": "long", "field-id": 512},
        {"name": "existing_rows_count", "type": "long", "field-id": 513},
        {"name": "deleted_rows_count", "type": "long", "field-id": 514},
        {"name": "partitions", "type": ["null", {"type": "array", "items": {"type": "record", "name": "r508", "fields": [
            {"name": "contains_null", "type": "boolean", "field-id": 509},
            {"name": "contains_nan", "type": ["null", "boolean"], "default": null, "field-id": 518},
            {"name": "lower_bound", "type": ["null", "bytes"], "default": null, "field-id": 510},
            {"name": "upper_bound", "type": ["null", "bytes"], "default": null, "field-id": 511}
        ]}, "element-id": 508}], "default": null, "field-id": 507},
        {"name": "key_metadata", "type": ["null", "bytes"], "default": null, "field-id": 519}
    ]}"#;

    fn i64_of(n: u64) -> i64 {
        i64::try_from(n).unwrap_or(i64::MAX)
    }

    fn long_map(m: &BTreeMap<i32, u64>) -> Value {
        some(Value::Array(
            m.iter()
                .map(|(k, v)| {
                    Value::Record(vec![("key".into(), Value::Int(*k)), ("value".into(), Value::Long(i64_of(*v)))])
                })
                .collect(),
        ))
    }

    fn bytes_map(m: &BTreeMap<i32, Vec<u8>>) -> Value {
        some(Value::Array(
            m.iter()
                .map(|(k, v)| {
                    Value::Record(vec![("key".into(), Value::Int(*k)), ("value".into(), Value::Bytes(v.clone()))])
                })
                .collect(),
        ))
    }

    fn opt<T>(v: Option<T>, f: impl FnOnce(T) -> Value) -> Value {
        v.map_or_else(null, |v| some(f(v)))
    }

    /// Writes a format v2 data manifest of `entries`. Added entries leave their sequence numbers to
    /// be inherited from the manifest list (`None` in `sequence_number` means the commit's own);
    /// existing and deleted entries keep theirs.
    pub fn write_manifest(ctx: &ManifestContext<'_>, entries: &[ManifestWrite]) -> Result<Vec<u8>> {
        let schema = manifest_schema(ctx.partition_type)?;
        let mut w = Writer::with_codec(&schema, vec![], Codec::Null).map_err(error)?;
        for (k, v) in [
            ("schema", ctx.schema_json.to_string()),
            ("schema-id", ctx.schema_id.to_string()),
            ("partition-spec", ctx.spec_fields_json.to_string()),
            ("partition-spec-id", ctx.spec_id.to_string()),
            ("format-version", "2".to_string()),
            ("content", "data".to_string()),
        ] {
            w.add_user_metadata(k.to_string(), v).map_err(error)?;
        }
        for e in entries {
            let f = &e.file;
            if f.content != Content::Data {
                return Err(Error::Unsupported("writing delete files".into()));
            }
            if f.partition.len() != ctx.partition_type.len() {
                return Err(Error::Invalid(format!(
                    "{}: {} partition values for a spec of {} fields",
                    f.path,
                    f.partition.len(),
                    ctx.partition_type.len()
                )));
            }
            let partition = ctx
                .partition_type
                .iter()
                .zip(&f.partition)
                .map(|(p, v)| {
                    Ok((avro_name(&p.name), opt(v.as_ref().map(|v| avro_value(v, &p.ty)).transpose()?, |v| v)))
                })
                .collect::<Result<Vec<_>>>()?;
            let data_file = Value::Record(vec![
                ("content".into(), Value::Int(f.content as i32)),
                ("file_path".into(), Value::String(f.path.clone())),
                ("file_format".into(), Value::String(f.file_format.clone())),
                ("partition".into(), Value::Record(partition)),
                ("record_count".into(), Value::Long(i64_of(f.record_count))),
                ("file_size_in_bytes".into(), Value::Long(i64_of(f.file_size))),
                ("column_sizes".into(), long_map(&f.column_sizes)),
                ("value_counts".into(), long_map(&f.value_counts)),
                ("null_value_counts".into(), long_map(&f.null_counts)),
                ("nan_value_counts".into(), long_map(&f.nan_counts)),
                ("lower_bounds".into(), bytes_map(&f.lower_bounds)),
                ("upper_bounds".into(), bytes_map(&f.upper_bounds)),
                ("key_metadata".into(), null()),
                (
                    "split_offsets".into(),
                    opt(f.split_offsets.as_ref(), |o| Value::Array(o.iter().map(|v| Value::Long(*v)).collect())),
                ),
                (
                    "equality_ids".into(),
                    opt(f.equality_ids.as_ref(), |ids| Value::Array(ids.iter().map(|v| Value::Int(*v)).collect())),
                ),
                ("sort_order_id".into(), opt(f.sort_order_id, Value::Int)),
                ("first_row_id".into(), opt(f.first_row_id, Value::Long)),
                ("referenced_data_file".into(), opt(f.referenced_data_file.clone(), Value::String)),
                ("content_offset".into(), opt(f.content_offset, Value::Long)),
                ("content_size_in_bytes".into(), opt(f.content_size, Value::Long)),
            ]);
            w.append_value(Value::Record(vec![
                ("status".into(), Value::Int(e.status as i32)),
                ("snapshot_id".into(), opt(e.snapshot_id, Value::Long)),
                ("sequence_number".into(), opt(e.sequence_number, Value::Long)),
                ("file_sequence_number".into(), opt(e.file_sequence_number, Value::Long)),
                ("data_file".into(), data_file),
            ]))
            .map_err(error)?;
        }
        w.into_inner().map_err(error)
    }

    /// Writes a format v2 manifest list for snapshot `snapshot_id` (child of `parent`) with sequence
    /// number `sequence_number`. Manifests with sequence number [`UNASSIGNED`] get the snapshot's
    /// (they were written by it).
    pub fn write_manifest_list(
        manifests: &[ManifestFile],
        snapshot_id: i64,
        parent: Option<i64>,
        sequence_number: i64,
    ) -> Result<Vec<u8>> {
        let schema = AvroSchema::parse_str(MANIFEST_LIST_SCHEMA).map_err(error)?;
        let mut w = Writer::with_codec(&schema, vec![], Codec::Null).map_err(error)?;
        let parent = parent.map_or_else(|| "null".into(), |p| p.to_string());
        for (k, v) in [
            ("snapshot-id", snapshot_id.to_string()),
            ("parent-snapshot-id", parent),
            ("sequence-number", sequence_number.to_string()),
            ("format-version", "2".to_string()),
        ] {
            w.add_user_metadata(k.to_string(), v).map_err(error)?;
        }
        for m in manifests {
            let assign = |v: i64| if v == UNASSIGNED { sequence_number } else { v };
            let partitions = opt(m.partitions.as_ref(), |fields| {
                Value::Array(
                    fields
                        .iter()
                        .map(|p| {
                            Value::Record(vec![
                                ("contains_null".into(), Value::Boolean(p.contains_null)),
                                ("contains_nan".into(), opt(p.contains_nan, Value::Boolean)),
                                ("lower_bound".into(), opt(p.lower.clone(), Value::Bytes)),
                                ("upper_bound".into(), opt(p.upper.clone(), Value::Bytes)),
                            ])
                        })
                        .collect(),
                )
            });
            w.append_value(Value::Record(vec![
                ("manifest_path".into(), Value::String(m.path.clone())),
                ("manifest_length".into(), Value::Long(m.length)),
                ("partition_spec_id".into(), Value::Int(m.partition_spec_id)),
                ("content".into(), Value::Int(m.content as i32)),
                ("sequence_number".into(), Value::Long(assign(m.sequence_number))),
                ("min_sequence_number".into(), Value::Long(assign(m.min_sequence_number))),
                ("added_snapshot_id".into(), Value::Long(m.added_snapshot_id)),
                ("added_files_count".into(), Value::Int(m.added_files)),
                ("existing_files_count".into(), Value::Int(m.existing_files)),
                ("deleted_files_count".into(), Value::Int(m.deleted_files)),
                ("added_rows_count".into(), Value::Long(m.added_rows)),
                ("existing_rows_count".into(), Value::Long(m.existing_rows)),
                ("deleted_rows_count".into(), Value::Long(m.deleted_rows)),
                ("partitions".into(), partitions),
                ("key_metadata".into(), null()),
            ]))
            .map_err(error)?;
        }
        w.into_inner().map_err(error)
    }

    /// The manifest-list entry of a data manifest written as `bytes` at `path` with `entries`.
    pub fn manifest_file(
        path: &str,
        bytes: &[u8],
        spec_id: i32,
        snapshot_id: i64,
        entries: &[ManifestWrite],
        partition_type: &[PartitionFieldType],
    ) -> ManifestFile {
        let count = |s: Status| i32::try_from(entries.iter().filter(|e| e.status == s).count()).unwrap_or(i32::MAX);
        let rows = |s: Status| i64_of(entries.iter().filter(|e| e.status == s).map(|e| e.file.record_count).sum());
        // Entries that inherit the commit's sequence number sort after every explicit one.
        let min_sequence_number = entries.iter().filter_map(|e| e.sequence_number).min().unwrap_or(UNASSIGNED);
        let partitions = (!partition_type.is_empty()).then(|| {
            partition_type
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let values = || entries.iter().map(|e| e.file.partition.get(i).and_then(Option::as_ref));
                    let floating = matches!(p.ty, PrimitiveType::Float | PrimitiveType::Double);
                    let ordered = || values().flatten().filter(|v| !v.is_nan());
                    let pick = |better: std::cmp::Ordering| {
                        ordered().fold(None::<&Datum>, |best, v| match best {
                            Some(b) if v.partial_cmp(b) != Some(better) => Some(b),
                            _ => Some(v),
                        })
                    };
                    FieldSummary {
                        contains_null: values().any(|v| v.is_none()),
                        contains_nan: floating.then(|| values().flatten().any(Datum::is_nan)),
                        lower: pick(std::cmp::Ordering::Less).map(Datum::to_bytes),
                        upper: pick(std::cmp::Ordering::Greater).map(Datum::to_bytes),
                    }
                })
                .collect()
        });
        ManifestFile {
            path: path.to_string(),
            length: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            partition_spec_id: spec_id,
            content: ManifestContent::Data,
            sequence_number: UNASSIGNED,
            min_sequence_number,
            added_snapshot_id: snapshot_id,
            added_files: count(Status::Added),
            existing_files: count(Status::Existing),
            deleted_files: count(Status::Deleted),
            added_rows: rows(Status::Added),
            existing_rows: rows(Status::Existing),
            deleted_rows: rows(Status::Deleted),
            partitions,
            first_row_id: None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn names_become_avro_names() {
            assert_eq!(avro_name("time_day"), "time_day");
            assert_eq!(avro_name("1st"), "_1st");
            assert_eq!(avro_name("a-b.c"), "a_x2Db_x2Ec");
        }
    }
}

#[cfg(all(test, feature = "write"))]
mod tests {
    use super::*;

    fn file(path: &str, day: Option<i32>, device: &str) -> Arc<DataFile> {
        Arc::new(DataFile {
            path: path.into(),
            partition: vec![day.map(Datum::Date), Some(Datum::String(device.into()))],
            record_count: 10,
            file_size: 100,
            column_sizes: BTreeMap::from([(1, 40)]),
            value_counts: BTreeMap::from([(1, 10)]),
            lower_bounds: BTreeMap::from([(1, 5i64.to_le_bytes().to_vec())]),
            upper_bounds: BTreeMap::from([(1, 9i64.to_le_bytes().to_vec())]),
            split_offsets: Some(vec![4]),
            sort_order_id: Some(1),
            ..DataFile::default()
        })
    }

    #[test]
    fn manifests_round_trip() {
        let schema = r#"{"schema-id":0,"type":"struct","fields":[
            {"id":1,"name":"time","required":true,"type":"timestamptz"},
            {"id":2,"name":"device","required":true,"type":"string"}]}"#;
        let spec = r#"[{"source-id":1,"field-id":1000,"name":"time_day","transform":"day"},
            {"source-id":2,"field-id":1001,"name":"device","transform":"identity"}]"#;
        let types = vec![
            PartitionFieldType { field_id: 1000, name: "time_day".into(), ty: PrimitiveType::Date },
            PartitionFieldType { field_id: 1001, name: "device".into(), ty: PrimitiveType::String },
        ];
        let ctx = ManifestContext {
            schema_json: schema,
            schema_id: 0,
            spec_fields_json: spec,
            spec_id: 0,
            partition_type: &types,
        };
        let entries = vec![
            ManifestWrite {
                status: Status::Added,
                snapshot_id: Some(7),
                sequence_number: None,
                file_sequence_number: None,
                file: file("a", Some(20_000), "b"),
            },
            ManifestWrite {
                status: Status::Existing,
                snapshot_id: Some(3),
                sequence_number: Some(2),
                file_sequence_number: Some(2),
                file: file("b", None, "a"),
            },
        ];
        let bytes = write_manifest(&ctx, &entries).unwrap();
        let mut mf = manifest_file("m.avro", &bytes, 0, 7, &entries, &types);
        assert_eq!((mf.added_files, mf.existing_files, mf.added_rows), (1, 1, 10));
        let summaries = mf.partitions.clone().unwrap();
        assert_eq!(
            summaries[0],
            FieldSummary {
                contains_null: true,
                contains_nan: None,
                lower: Some(20_000i32.to_le_bytes().to_vec()),
                upper: Some(20_000i32.to_le_bytes().to_vec())
            }
        );
        assert_eq!((summaries[1].lower.as_deref(), summaries[1].upper.as_deref()), (Some(&b"a"[..]), Some(&b"b"[..])));

        let list = write_manifest_list(std::slice::from_ref(&mf), 7, Some(3), 5).unwrap();
        let read = read_manifest_list(&list).unwrap();
        // The list assigns the snapshot's sequence number; the minimum is the explicit 2.
        mf.sequence_number = 5;
        assert_eq!(read, [mf.clone()]);

        let read = read_manifest(&mf, &bytes).unwrap();
        assert_eq!((read[0].sequence_number, read[0].snapshot_id, read[0].status), (5, 7, Status::Added));
        assert_eq!((read[1].sequence_number, read[1].snapshot_id, read[1].status), (2, 3, Status::Existing));
        assert_eq!(read[0].file, entries[0].file);
        assert_eq!(read[1].file, entries[1].file);
    }

    #[test]
    fn partition_values_must_fit_the_spec() {
        let types = vec![PartitionFieldType { field_id: 1000, name: "d".into(), ty: PrimitiveType::Date }];
        let ctx = ManifestContext {
            schema_json: "{}",
            schema_id: 0,
            spec_fields_json: "[]",
            spec_id: 0,
            partition_type: &types,
        };
        let write = |partition: Vec<Option<Datum>>| {
            let file = Arc::new(DataFile { path: "a".into(), partition, ..DataFile::default() });
            let entry = ManifestWrite {
                status: Status::Added,
                snapshot_id: None,
                sequence_number: None,
                file_sequence_number: None,
                file,
            };
            write_manifest(&ctx, &[entry])
        };
        assert!(write(vec![Some(Datum::Date(1))]).is_ok());
        assert!(matches!(write(vec![Some(Datum::Int(1))]), Err(crate::Error::Invalid(_))));
        assert!(matches!(write(vec![]), Err(crate::Error::Invalid(_))));
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(read_manifest_list(b"not avro").is_err());
        assert!(read_manifest_list(&[]).is_err());
    }
}
