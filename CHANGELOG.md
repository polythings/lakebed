# Changelog

All notable changes to lakekit. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Added

- `lakekit-core`: the `Host` trait for object stores, clocks, and randomness; `MemoryHost`; one `Error` type.
- `lakekit-iceberg`: Iceberg table metadata, schemas, partition specs, manifest lists, and manifests read for format
  versions 1 to 3, including delete files and deletion vector references; values in Iceberg's single-value encoding.
- `lakekit-iceberg` feature `write`: format v2 manifests, manifest lists, and metadata updates, and commits to
  file-system tables with options (`CommitOptions`) and hooks (`CommitHooks`). A writer whose clock is behind the
  table's last update dates its commit at that update, so the snapshot log stays in order.
- `lakekit-iceberg` features `arrow` (schemas as Arrow schemas with field ids) and `parquet-stats` (a data file's
  Iceberg statistics from its Parquet footer; a bound or null count only when every row group gives it exactly, and
  decimals as Iceberg's shortest big-endian bytes).
- `lakekit`: re-exports the format crates behind a feature each.

The Iceberg code started as Timelake's shared writer (`timelake-format`), made general.
