# Changelog

All notable changes to lakekit. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

## 0.1.0 (2026-10-22)

### Added

- `lakekit-core`: loko's `Store`, `Clock`, and `Random` services with their test and native implementations
  (`TokioClock` with feature `tokio`, `OsRandom` with feature `getrandom`); `Services`, the three borrowed
  together for commits; one `Error` type, which a store's error converts into.
- `lakekit` feature `object-store`: a `Store` over an `object_store` client (S3, GCS, Azure, local files, memory),
  from a client or a URL (`loko-object-store`).
- `lakekit-iceberg`: Iceberg table metadata, schemas, partition specs, manifest lists, and manifests read for format
  versions 1 to 3, including delete files and deletion vector references; values in Iceberg's single-value encoding.
- `lakekit-iceberg` feature `write`: format v2 manifests, manifest lists, and metadata updates, and commits to
  file-system tables with options (`CommitOptions`) and hooks (`CommitHooks`). A writer whose clock is behind the
  table's last update dates its commit at that update, so the snapshot log stays in order.
- `lakekit-iceberg` features `arrow` (schemas as Arrow schemas with field ids) and `parquet-stats` (a data file's
  Iceberg statistics from its Parquet footer; a bound or null count only when every row group gives it exactly, and
  decimals as Iceberg's shortest big-endian bytes).
- `lakekit`: re-exports the format crates behind a feature each.

### Changed

- The project is named lakekit, and its crates `lakekit-core`, `lakekit-iceberg`, and `lakekit`; it was lakebed
  (`lakebed-core`, …) until 2026-10-21. Nothing was published under the old names; GitHub redirects the old
  repository URL.
- The I/O boundary is three traits, `Store` (objects), `Clock` (the time and a timer), and `Random` (random numbers),
  instead of one `Host`; `MemoryHost` is `MemoryServices`. UUIDs come from the `uuid` crate.
- The three traits are [loko](https://github.com/polythings/loko)'s, and `Clock::sleep` takes a `Duration`. The
  `object_store` adapter is `loko-object-store`, re-exported as `lakekit::object_store`; `lakekit-object-store` is
  gone (it was never published).

The Iceberg code started as Timelake's shared writer (`timelake-format`), made general.
