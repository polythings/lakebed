---
number: 3
title: Apache Iceberg
status: in-progress
updated: 2026-10-20
versions: [unreleased]
related: [1, 2]
issues: [1, 2, 3, 4, 5]
---

# 3. Apache Iceberg

**In progress** · updated 2026-10-20 · versions unreleased · milestone: [0.1](milestones/v0-1.md)

## Summary

`lakekit-iceberg` reads Iceberg tables of format versions 1 to 3 and writes format v2 tables, without I/O of its own
(spec 1). Timelake's WASM reader and writer use it, and lakeglass's table explorer will. It started as Timelake's
shared writer, made general.

## Design

**Built:**

- **Types and values.** Iceberg's primitive types, nested types (struct, list, map), and types this version doesn't
  model (format v3's `variant`, `geometry`, …) kept by name so such tables still read. `Datum` holds a value of any
  primitive type, with Iceberg's single-value binary encoding both ways; a `long` or `double` bound stored in 4 bytes
  (written before the column was promoted) widens.
- **Schemas and partition specs**, from metadata JSON: field lookup by id at any depth, transforms (`identity`,
  `bucket[N]`, `truncate[W]`, `year`, `month`, `day`, `hour`, `void`, and unknown ones by name), and each partition
  field's result type (`day` gives a `date`, as iceberg-rust types it). Format v1 specs without field ids get 1000
  and up.
- **Table metadata** (`Metadata`): kept as JSON, so fields this crate doesn't model survive a rewrite; typed views of
  snapshots, ancestors, the current schema and spec (including format v1's `schema` and `partition-spec`), and
  properties. Format versions above 3 are refused.
- **Manifest lists and manifests,** format v1 to v3: data and delete files (position, equality, and v3 deletion
  vectors' `referenced_data_file`, `content_offset`, and `content_size_in_bytes`), column statistics, and partition
  values typed from the manifest header's schema and spec. Entries inherit snapshot ids and sequence numbers from
  their manifest, as the spec says. Format v1's `*_data_files_count` names are read.
- **Tables** (`Table`): the newest `v<N>.metadata.json` by listing, or `version-hint.text` where nothing is listed;
  any metadata file a catalog names (`from_metadata_file`); manifests and live files.
- **Writing (feature `write`):** format v2 data manifests and manifest lists in the layout iceberg-rust writes, and
  metadata updates (`with_snapshot`, `without_snapshots`) made as iceberg-rust's builder makes them, with the metadata
  log trimmed to `write.metadata.previous-versions-max`; the current snapshot can't be removed. Partition values of
  type uuid, fixed, and decimal are refused for now.
- **Commits (feature `write`):** upload a snapshot's files, then write `v<N+1>.metadata.json` only if absent; on a
  lost race, reload and try again on the newer version, deleting the lost attempt's files; when the outcome is
  unknown (a conditional put that failed and can't be read back), delete nothing. Appends, replacements that keep data
  sequence numbers (compaction, one sequence number per added file), and deletions; manifests merge once there would be
  more than `merge_manifests_at`. Iceberg's summary counters are kept. A writer whose clock is behind the table's last update dates its commit at that
  update, so the snapshot log stays in order (iceberg-rust refuses tables whose log isn't).
- **Options and hooks:** `CommitOptions` (`max_attempts` 10, `backoff_ms` 5–50 times the attempt, `merge_manifests_at`
  8, `write_version_hint` on) and `CommitHooks` (`reload`, `skip` for idempotent retries, `extra` for files and
  summary properties of the application's own). Timelake's last-value index, batch ids, and head use them.
- **Arrow (feature `arrow`):** the schema as an Arrow schema with field ids under `PARQUET:field_id`.
- **Statistics (feature `parquet-stats`):** a data file's counts, bounds, and split offsets from its Parquet footer, as
  iceberg-rust's writer records them; decimals become Iceberg's shortest big-endian bytes however Parquet stores them.
  A bound or null count is recorded only when every row group gives it exactly (a row group of nulls only needs no
  bounds): readers take them as covering the whole file, and a partial one would let them skip matching rows.
- **Interop tests:** iceberg-rust creates a table and appends; lakekit reads it and commits; iceberg-rust reads that
  version, parses lakekit's manifests, and appends on top; lakekit reads all of it. lakekit's statistics of
  iceberg-rust's Parquet file equal what iceberg-rust recorded.

**Not yet:** applying partition transforms to values, planning files from filters, writing format v3, writing delete
files, and catalogs (REST). Each has an issue.

## Decisions

| ID | Decision | Status |
|---|---|---|
| D7 | Read format v1 to v3; write format v2 first, the version every reader supports | Accepted |
| D8 | Table metadata stays JSON with typed views, so unknown fields survive rewrites | Accepted |
| D9 | Commits are extended through hooks (`CommitHooks`) rather than by copying the protocol | Accepted |

## Open questions and issues

- [#1](https://github.com/polythings/lakekit/issues/1) Apply partition transforms to values
- [#2](https://github.com/polythings/lakekit/issues/2) Plan files and row groups from filters
- [#3](https://github.com/polythings/lakekit/issues/3) Write uuid, fixed and decimal partition values
- [#4](https://github.com/polythings/lakekit/issues/4) Write format v3 tables
- [#5](https://github.com/polythings/lakekit/issues/5) Iceberg REST catalogs: here or in lakeglass?

## History

- 2026-10-20: built from Timelake's shared writer (`timelake-format`), made general: any partition spec, format v1 to
  v3 reads, delete files, hooks, and options.
