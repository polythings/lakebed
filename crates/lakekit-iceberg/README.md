# lakekit-iceberg

[Apache Iceberg](https://iceberg.apache.org) tables read and written without I/O of their own, through a
[`lakekit_core::Host`](../lakekit-core), so they work in WebAssembly (browsers, Cloudflare Workers) as well as natively.

| Feature | What it adds |
|---|---|
| (always) | Table metadata of format v1 to v3, schemas, partition specs, snapshots; manifest lists and manifests with data and delete files; values in Iceberg's single-value encoding; loading a table from its location or a metadata file |
| `write` | Format v2 manifests, manifest lists, and metadata updates; commits to file-system tables with `CommitOptions` and `CommitHooks` |
| `arrow` | The schema as an Arrow schema, with field ids |
| `parquet-stats` | A data file's Iceberg statistics (counts, bounds, split offsets) from its Parquet footer |

The tests commit to one table with lakekit and iceberg-rust in turn, each reading what the other wrote.

Apache License 2.0.
