# lakekit

Table formats read and written without I/O of their own, in Rust, for WebAssembly and native targets. Each format's
metadata (Apache Iceberg first) is parsed and written by plain code that asks a `Host` for bytes: the application
implements the host over its object store, so the same crate runs in a browser, a Cloudflare Worker, a CLI, or a
server.

lakekit is what [Timelake](https://github.com/polythings/timelake) and [lakeglass](https://github.com/polythings/lakeglass)
share. The official Rust crates for these formats build on tokio and HTTP clients, which don't compile to
`wasm32-unknown-unknown`.

## Crates

| Crate | What it does |
|---|---|
| [`lakekit-core`](crates/lakekit-core) | The `Host` trait (ranged reads, listing, conditional writes, clock, randomness), errors, an in-memory host |
| [`lakekit-iceberg`](crates/lakekit-iceberg) | Apache Iceberg: metadata, manifests, and commits |
| [`lakekit`](crates/lakekit) | Re-exports the format crates, one feature each |

More formats get their own crate as they are added (spec 2).

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
use lakekit_iceberg::Table;

async fn files<H: lakekit_core::Host>(host: &H) -> lakekit_core::Result<usize> {
    let mut table = Table::load(host, "s3://bucket/warehouse/db/events").await?;
    Ok(table.live_files(host).await?.len())
}
```

## Development

```bash
cargo test --workspace --all-features                                        # tests, offline
cargo build --workspace --all-features --target wasm32-unknown-unknown       # the WebAssembly build
prek run --all-files --hook-stage pre-push                                   # formatters and linters
```

[CONTRIBUTING.md](CONTRIBUTING.md) is the code guide, and [spec/](spec/README.md) holds the design.

## License

Apache License 2.0 ([LICENSE](LICENSE)).
