# Changelog

All notable changes to lakebed. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- `lakebed-core`: the `Host` trait for object stores, clocks, and randomness; `MemoryHost`; one `Error` type.
- `lakebed-iceberg`: Iceberg table metadata, schemas, partition specs, manifest lists, and manifests read for format
  versions 1 to 3, including delete files and deletion vector references; values in Iceberg's single-value encoding.
- `lakebed-iceberg` feature `write`: format v2 manifests, manifest lists, and metadata updates, and commits to
  file-system tables with options (`CommitOptions`) and hooks (`CommitHooks`). A writer whose clock is behind the
  table's last update dates its commit at that update, so the snapshot log stays in order.
- `lakebed-iceberg` features `arrow` (schemas as Arrow schemas with field ids) and `parquet-stats` (a data file's
  Iceberg statistics from its Parquet footer).
- `lakebed`: re-exports the format crates behind a feature each.

The Iceberg code started as Timelake's shared writer (`timelake-format`), made general.
