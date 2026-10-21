# Prior art

lakebed stands on other open-source projects and learns from more. This file credits the ideas we took. We take ideas,
never code: every line of lakebed is our own. How we credit is in [CONTRIBUTING.md](CONTRIBUTING.md#credit).

## Foundations

| Project | License | What lakebed uses |
|---|---|---|
| [Apache Iceberg](https://iceberg.apache.org) | Apache-2.0 | The table format and its specification |
| [iceberg-rust](https://github.com/apache/iceberg-rust) | Apache-2.0 | The reference the interop tests check against (a test dependency only) |
| [Apache Avro](https://avro.apache.org) (`apache-avro`) | Apache-2.0 | Reading and writing manifests and manifest lists |
| [arrow-rs](https://github.com/apache/arrow-rs) (`arrow-schema`, `parquet`) | Apache-2.0 | Arrow schemas; Parquet footers for statistics |

## Ideas

| Idea | From | License of the source | Where in lakebed |
|---|---|---|---|
| A format library with no I/O of its own: the application supplies storage through a trait the library calls | [delta-kernel-rs](https://github.com/delta-io/delta-kernel-rs) (its `Engine` trait) | Apache-2.0 | `lakebed-core`'s `Host` |
| One crate per backend under a small shared core, re-exported by an umbrella crate with a feature each | [arrow-rs](https://github.com/apache/arrow-rs), [Apache OpenDAL](https://github.com/apache/opendal) | Apache-2.0 | The workspace layout |
| Commits to tables without a catalog: `v<N>.metadata.json` written only if absent, `version-hint.text` for readers | Iceberg's file-system ("Hadoop") tables | Apache-2.0 | `lakebed_iceberg::commit` |
| Avro-safe names for partition fields (`_x` and the character's code in hex) | Iceberg's Java implementation | Apache-2.0 | Manifest writing |
