//! lakekit-iceberg and iceberg-rust read each other's tables: a table iceberg-rust creates and
//! appends to, lakekit reads and commits to; iceberg-rust reads lakekit's commit and appends on top
//! of it; and lakekit computes the statistics iceberg-rust's Parquet writer records. Everything
//! runs in memory.
#![cfg(all(feature = "write", feature = "arrow", feature = "parquet-stats"))]
#![expect(clippy::unwrap_used, reason = "test code")]

use std::collections::{BTreeMap, HashMap};
use std::future::{Future, ready};
use std::sync::Arc;

use arrow_array::{Float64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use bytes::Bytes;
use futures::TryStreamExt;
use iceberg::io::FileIO;
use iceberg::memory::MemoryCatalogBuilder;
use iceberg::spec::{
    DataFile as IceDataFile, FormatVersion, Literal, Manifest, ManifestList, NestedField, PrimitiveType as IceType,
    Schema as IceSchema, Struct, Transform as IceTransform, Type as IceFieldType, UnboundPartitionSpec,
};
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::writer::file_writer::{FileWriter as _, FileWriterBuilder, ParquetWriterBuilder};
use iceberg::{Catalog, CatalogBuilder, MemoryCatalog, NamespaceIdent, TableCreation, TableIdent};
use lakekit_core::{Error, Listed, MemoryStore, MemoryUpload, Services, Store};
use lakekit_iceberg::stats::ParquetStats;
use lakekit_iceberg::{Change, CommitOptions, DataFile, Datum, Entry, NoHooks, Status, Table, commit};
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::ParquetMetaDataReader;
use parquet::file::properties::WriterProperties;

/// Microseconds at 2026-01-01T00:00:00Z.
const T0: i64 = 1_767_225_600_000_000;
const DAY_US: i64 = 86_400_000_000;

/// A [`Store`] over iceberg-rust's `FileIO`, so both see the same objects. Listing isn't needed
/// (tables load from their metadata file).
struct IoStore {
    io: FileIO,
}

/// The store with a [`MemoryStore`]'s clock and random numbers.
type TestStore = Services<IoStore, MemoryStore, MemoryStore>;

fn store(e: impl std::fmt::Display) -> Error {
    Error::Store(e.to_string())
}

impl Store for IoStore {
    type Upload = MemoryUpload;

    async fn get(&self, uri: &str) -> lakekit_core::Result<Option<Vec<u8>>> {
        if !self.io.exists(uri).await.map_err(store)? {
            return Ok(None);
        }
        Ok(Some(self.io.new_input(uri).map_err(store)?.read().await.map_err(store)?.to_vec()))
    }

    async fn get_range(&self, uri: &str, start: u64, len: u64) -> lakekit_core::Result<Vec<u8>> {
        let bytes = self.get(uri).await?.ok_or_else(|| Error::NotFound(uri.to_string()))?;
        let start = usize::try_from(start).map_err(store)?.min(bytes.len());
        let end = start.saturating_add(usize::try_from(len).map_err(store)?).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    async fn exists(&self, uri: &str) -> lakekit_core::Result<bool> {
        self.io.exists(uri).await.map_err(store)
    }

    async fn put(&self, uri: &str, bytes: Vec<u8>) -> lakekit_core::Result<()> {
        self.io.new_output(uri).map_err(store)?.write(Bytes::from(bytes)).await.map_err(store)
    }

    /// The tests run one writer at a time, so checking first is enough.
    async fn put_if_absent(&self, uri: &str, bytes: Vec<u8>) -> lakekit_core::Result<bool> {
        if self.exists(uri).await? {
            return Ok(false);
        }
        self.put(uri, bytes).await.map(|()| true)
    }

    fn upload(&self, _: &str) -> impl Future<Output = lakekit_core::Result<MemoryUpload>> {
        ready(Err(Error::Unsupported("uploads".into())))
    }

    fn list(&self, _: &str) -> impl Future<Output = lakekit_core::Result<Vec<Listed>>> {
        ready(Err(Error::Unsupported("listing".into())))
    }

    async fn delete(&self, uris: &[String]) -> lakekit_core::Result<()> {
        for uri in uris {
            self.io.delete(uri).await.map_err(store)?;
        }
        Ok(())
    }
}

fn ice_schema() -> IceSchema {
    IceSchema::builder()
        .with_schema_id(0)
        .with_fields(vec![
            NestedField::required(1, "time", IceFieldType::Primitive(IceType::Timestamptz)).into(),
            NestedField::required(2, "device", IceFieldType::Primitive(IceType::String)).into(),
            NestedField::optional(3, "value", IceFieldType::Primitive(IceType::Double)).into(),
        ])
        .build()
        .unwrap()
}

/// Rows of one day: `n` readings from `day` days after `T0`, every seventh value NaN.
fn batch(arrow: &arrow_schema::SchemaRef, day: i64, n: i64) -> RecordBatch {
    let start = T0 + day * DAY_US;
    let time =
        TimestampMicrosecondArray::from_iter_values((0..n).map(|i| start + i * 1_000_000)).with_timezone("+00:00");
    let device = StringArray::from_iter_values((0..n).map(|i| format!("d{}", i % 5)));
    let value: Float64Array = (0..n)
        .map(|i| {
            if i % 7 == 0 {
                Some(f64::NAN)
            } else if i % 11 == 0 {
                None
            } else {
                Some(f64::from(i32::try_from(i).unwrap()))
            }
        })
        .collect();
    RecordBatch::try_new(arrow.clone(), vec![Arc::new(time), Arc::new(device), Arc::new(value)]).unwrap()
}

fn nan_counts(batch: &RecordBatch) -> BTreeMap<i32, u64> {
    let values = batch.column(2).as_any().downcast_ref::<Float64Array>().unwrap();
    BTreeMap::from([(3, values.iter().filter(|v| v.is_some_and(f64::is_nan)).count() as u64)])
}

/// Writes `batch` with iceberg-rust's Parquet writer; returns its data file.
async fn write_theirs(table: &iceberg::table::Table, batch: &RecordBatch, name: &str, day: i32) -> IceDataFile {
    let builder = ParquetWriterBuilder::new(WriterProperties::default(), table.metadata().current_schema().clone());
    let uri = format!("{}/data/{name}.parquet", table.metadata().location());
    let mut writer = builder.build(table.file_io().new_output(&uri).unwrap()).await.unwrap();
    writer.write(batch).await.unwrap();
    let mut files = writer.close().await.unwrap();
    files.remove(0).partition(Struct::from_iter([Some(Literal::int(day))])).partition_spec_id(0).build().unwrap()
}

/// Writes `batch` with arrow-rs's Parquet writer; returns lakekit's data file.
async fn write_ours(store: &TestStore, table: &Table, batch: &RecordBatch, name: &str, day: i32) -> DataFile {
    let mut bytes = vec![];
    let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), None).unwrap();
    writer.write(batch).unwrap();
    let footer = writer.close().unwrap();
    let stats = ParquetStats::new(&footer, bytes.len() as u64, nan_counts(batch));
    let uri = format!("{}/data/{name}.parquet", table.location);
    store.put(&uri, bytes).await.unwrap();
    stats.data_file(uri, vec![Some(Datum::Date(day))])
}

async fn append_theirs(
    catalog: &MemoryCatalog,
    table: &iceberg::table::Table,
    file: IceDataFile,
) -> iceberg::table::Table {
    let tx = Transaction::new(table);
    let tx = tx.fast_append().add_data_files([file]).apply(tx).unwrap();
    tx.commit(catalog).await.unwrap()
}

/// The data file iceberg-rust records, as lakekit records it.
fn as_ours(f: &IceDataFile) -> DataFile {
    let bytes =
        |m: &HashMap<i32, iceberg::spec::Datum>| m.iter().map(|(k, d)| (*k, d.to_bytes().unwrap().to_vec())).collect();
    let counts = |m: &HashMap<i32, u64>| m.iter().map(|(k, v)| (*k, *v)).collect();
    let partition = f
        .partition()
        .iter()
        .map(|v| match v {
            Some(Literal::Primitive(iceberg::spec::PrimitiveLiteral::Int(d))) => Some(Datum::Date(*d)),
            other => panic!("partition value {other:?}"),
        })
        .collect();
    DataFile {
        path: f.file_path().to_string(),
        partition,
        record_count: f.record_count(),
        file_size: f.file_size_in_bytes(),
        column_sizes: counts(f.column_sizes()),
        value_counts: counts(f.value_counts()),
        null_counts: counts(f.null_value_counts()),
        nan_counts: counts(f.nan_value_counts()),
        lower_bounds: bytes(f.lower_bounds()),
        upper_bounds: bytes(f.upper_bounds()),
        split_offsets: f.split_offsets().map(<[i64]>::to_vec),
        sort_order_id: f.sort_order_id(),
        ..DataFile::default()
    }
}

/// Files iceberg-rust plans for a scan of the table at `metadata_file`.
async fn planned(io: &FileIO, metadata_file: &str) -> Vec<String> {
    let ident = TableIdent::from_strs(["ns", "static"]).unwrap();
    let table = iceberg::table::StaticTable::from_metadata_file(metadata_file, ident, io.clone()).await.unwrap();
    let tasks: Vec<_> = table.scan().build().unwrap().plan_files().await.unwrap().try_collect().await.unwrap();
    let mut paths: Vec<String> = tasks.into_iter().map(|t| t.data_file_path).collect();
    paths.sort();
    paths
}

async fn live(store: &TestStore, metadata_file: &str) -> (Table, Vec<Entry>) {
    let mut table = Table::from_metadata_file(store, metadata_file).await.unwrap();
    let mut files = table.live_files(store).await.unwrap().to_vec();
    files.sort_by(|a, b| a.file.path.cmp(&b.file.path));
    (table, files)
}

#[tokio::test]
async fn tables_round_trip_between_lakekit_and_iceberg_rust() {
    let catalog = MemoryCatalogBuilder::default()
        .load("memory", HashMap::from([("warehouse".to_string(), "memory://wh".to_string())]))
        .await
        .unwrap();
    let ns = NamespaceIdent::new("ns".into());
    catalog.create_namespace(&ns, HashMap::new()).await.unwrap();
    let mut creation = TableCreation::builder()
        .name("t".into())
        .location("memory://wh/ns/t".into())
        .schema(ice_schema())
        .format_version(FormatVersion::V2)
        .build();
    creation.partition_spec =
        Some(UnboundPartitionSpec::builder().add_partition_field(1, "time_day", IceTransform::Day).unwrap().build());
    let table = catalog.create_table(&ns, creation).await.unwrap();
    let io = table.file_io().clone();
    let memory = MemoryStore::new(T0 / 1000);
    let store = Services::new(IoStore { io: io.clone() }, memory.clone(), memory);
    let arrow = Arc::new(iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema()).unwrap());

    // iceberg-rust appends; lakekit reads the file as iceberg-rust records it.
    let first = batch(&arrow, 0, 5_000);
    let theirs = write_theirs(&table, &first, "theirs-1", 20_454).await;
    let table = append_theirs(&catalog, &table, theirs.clone()).await;
    let (base, files) = live(&store, table.metadata_location().unwrap()).await;
    assert_eq!(files.len(), 1);
    assert_eq!(*files[0].file, as_ours(&theirs));
    assert_eq!((files[0].status, files[0].sequence_number, files[0].file_sequence_number), (Status::Added, 1, 1));

    // lakekit computes the statistics iceberg-rust's writer recorded, from the file's footer.
    let bytes = io.new_input(theirs.file_path()).unwrap().read().await.unwrap();
    let footer = ParquetMetaDataReader::new().parse_and_finish(&bytes).unwrap();
    let stats = ParquetStats::new(&footer, bytes.len() as u64, nan_counts(&first));
    assert_eq!(stats.data_file(theirs.file_path().into(), vec![Some(Datum::Date(20_454))]), as_ours(&theirs));

    // lakekit commits on top; iceberg-rust reads the new version and lakekit's manifests.
    let ours = write_ours(&store, &base, &batch(&arrow, 1, 3_000), "ours-1", 20_455).await;
    let (committed, info) = commit(
        &store,
        base,
        &Change::Append { files: vec![ours.clone()] },
        &HashMap::new(),
        &CommitOptions::default(),
        &mut NoHooks,
    )
    .await
    .unwrap();
    assert_eq!(info.version, 2);
    let paths = planned(&io, &committed.metadata_file).await;
    assert_eq!(paths, [ours.path.clone(), theirs.file_path().to_string()]);
    let list = io.new_input(committed.metadata.manifest_list().unwrap()).unwrap().read().await.unwrap();
    let list = ManifestList::parse_with_version(&list, FormatVersion::V2).unwrap();
    let added = list.entries().iter().find(|m| m.added_snapshot_id == info.snapshot_id).unwrap();
    assert_eq!((added.sequence_number, added.added_files_count), (2, Some(1)));
    let manifest = io.new_input(&added.manifest_path).unwrap().read().await.unwrap();
    let (entries, _) = Manifest::parse_avro(&manifest).unwrap().into_parts();
    assert_eq!(as_ours(&entries[0].data_file), ours);

    // iceberg-rust appends on top of lakekit's version; lakekit reads all three files. Its memory
    // catalog only names metadata files `NNNNN-<uuid>.metadata.json`, so it gets a copy so named.
    let ident = TableIdent::from_strs(["ns", "registered"]).unwrap();
    let copy =
        committed.metadata_file.replace("v2.metadata.json", "00002-1d5e8c1a-0000-4000-8000-000000000000.metadata.json");
    store.put(&copy, committed.metadata.to_json().into_bytes()).await.unwrap();
    let registered = catalog.register_table(&ident, copy).await.unwrap();
    let again = write_theirs(&registered, &batch(&arrow, 2, 1_000), "theirs-2", 20_456).await;
    let registered = append_theirs(&catalog, &registered, again.clone()).await;
    let (_, files) = live(&store, registered.metadata_location().unwrap()).await;
    let seen: Vec<(&str, i64)> =
        files.iter().map(|e| (e.file.path.rsplit('/').next().unwrap(), e.sequence_number)).collect();
    assert_eq!(seen, [("ours-1.parquet", 2), ("theirs-1.parquet", 1), ("theirs-2.parquet", 3)]);
    assert_eq!(*files[2].file, as_ours(&again));
}
