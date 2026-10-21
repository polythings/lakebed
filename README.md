# lakekit

Table formats read and written without I/O of their own, in Rust, for WebAssembly and native targets. Each format's
metadata (Apache Iceberg first) is parsed and written by plain code that asks a `Store` for bytes, a `Clock` for the
time, and a `Random` for random numbers: the application supplies them, so the same crate runs in a browser, a
Cloudflare Worker, a CLI, or a server.

lakekit is what [Timelake](https://github.com/polythings/timelake) and [lakeglass](https://github.com/polythings/lakeglass)
share. The official Rust crates for these formats build on tokio and HTTP clients, which don't compile to
`wasm32-unknown-unknown`.

## Crates

| Crate | What it does |
|---|---|
| [`lakekit-core`](crates/lakekit-core) | The `Store`, `Clock`, and `Random` traits, errors, an in-memory store, a tokio clock, OS randomness |
| [`lakekit-iceberg`](crates/lakekit-iceberg) | Apache Iceberg: metadata, manifests, and commits |
| [`lakekit-object-store`](crates/lakekit-object-store) | A `Store` over an `object_store` client: S3, GCS, Azure, local files, memory |
| [`lakekit`](crates/lakekit) | Re-exports the crates, one feature each |

More formats get their own crate as they are added (spec 2), and more stores their own adapter (spec 4).

## Iceberg

| Part | Status |
|---|---|
| Table metadata, format v1 to v3: schemas, partition specs, snapshots, properties | Read |
| Manifest lists and manifests: data files, delete files, deletion vector references, column statistics | Read |
| Format v2 manifests, manifest lists, and metadata updates | Write (feature `write`) |
| Commits to file-system tables (`v<N>.metadata.json` with a conditional put), with hooks | Write (feature `write`) |
| Schema as an Arrow schema with field ids | Feature `arrow` |
| A data file's Iceberg statistics from its Parquet footer | Feature `parquet-stats` |

What lakekit writes, iceberg-rust reads, and the other way round: the tests commit to one table with both in turn.

```rust
use std::sync::Arc;

use lakekit_iceberg::Table;
use lakekit_object_store::ObjectStoreAdapter;

async fn files(s3: Arc<dyn object_store::ObjectStore>) -> lakekit_core::Result<usize> {
    let store = ObjectStoreAdapter::new(s3, "s3://bucket");
    let mut table = Table::load(&store, "s3://bucket/warehouse/db/events").await?;
    Ok(table.live_files(&store).await?.len())
}
```

Commits also need a `Clock` and a `Random`: `Services::new(store, TokioClock, OsRandom)` natively, the application's
own in WebAssembly.

## Development

```bash
cargo test --workspace --all-features                                        # tests, offline
cargo build --workspace --all-features --target wasm32-unknown-unknown       # the WebAssembly build
prek run --all-files --hook-stage pre-push                                   # formatters and linters
```

[CONTRIBUTING.md](CONTRIBUTING.md) is the code guide, and [spec/](spec/README.md) holds the design.

## License

Apache License 2.0 ([LICENSE](LICENSE)).
