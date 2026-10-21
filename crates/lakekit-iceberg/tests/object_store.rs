//! An Iceberg table committed and read back through `object_store` (`loko-object-store`), with the
//! system clock and random numbers.
#![cfg(feature = "write")]

use std::collections::HashMap;
use std::sync::Arc;

use lakekit_core::{OsRandom, Services, Store, TokioClock};
use lakekit_iceberg::{Change, CommitOptions, DataFile, Datum, Metadata, NoHooks, Table, commit};
use loko_object_store::ObjectStoreAdapter;
use object_store::memory::InMemory;

#[tokio::test]
async fn iceberg_commits_through_object_store() {
    let store = ObjectStoreAdapter::new(Arc::new(InMemory::new()), "memory://");
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
    let (_, info) = commit(
        Services::new(&store, &TokioClock, &OsRandom),
        table,
        &change,
        &HashMap::new(),
        &CommitOptions::default(),
        &mut NoHooks,
    )
    .await
    .unwrap();
    assert_eq!(info.version, 2);

    let mut loaded = Table::load(&store, location).await.unwrap();
    assert_eq!(loaded.version, 2);
    assert_eq!(loaded.live_files(&store).await.unwrap()[0].file.record_count, 3);
}
