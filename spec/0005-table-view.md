---
number: 5
title: A common table view
status: planned
updated: 2026-10-21
versions: []
related: [2, 3, 6]
issues: [13, 16, 17]
---

# 5. A common table view

**Planned** · updated 2026-10-21 · milestone: [0.2](milestones/v0-2.md)

## Summary

`lakekit-table` is one view of a table whatever its format: its schema, properties and snapshots, its live data
files with their statistics, and a commit that adds and removes files. Applications that don't care which format a
table is (Timelake's engine, lakeglass's explorer and engine) are written against it once. The format crates stay as
they are (spec 2, D5); this view sits above them, with a feature per format, Iceberg first.

## Design

**What the view holds,** for one version of a table:

- **Location and version.** The version is the format's own (Iceberg's metadata version, Delta's log version),
  opaque to callers except for ordering.
- **Schema** as an Arrow schema with field ids, and **properties** as a string map.
- **Snapshots:** id, parent, time and summary map, newest last. Formats with branches and tags (Iceberg) list them
  too; others have none.
- **Live data files** (`DataFile`):
  - path, size and row count;
  - file format (Iceberg's `file_format` name: `PARQUET`, `AVRO`, `ORC`, or others such as `VORTEX` once a format
    lists them; spec 6);
  - per-column statistics by field id (min, max, null count);
  - partition values with their source column and transform;
  - the **commit rank**, the order in which files were added (Iceberg's data sequence number, Delta's log version, a
    DuckLake snapshot id);
  - the format's footer, when a reader needs it, as opaque bytes (spec 6).
- **Delete files** are listed apart, so an application that can't apply them can refuse the table.

**Operations,** all through loko's `Store`, `Clock` and `Random` (spec 1), with no I/O of their own:

- `load(location)` and `load_version(location, version)`.
- `create(location, schema, partitioning, properties)`.
- `files(snapshot)`: the live files of a snapshot, the newest by default.
- `commit(base, change, hooks, options)`:
  - The change adds files and removes files.
  - It sets or removes properties, and adds summary entries.
  - Its kind is append, replace (keeping the replaced rows' commit ranks, as compaction needs) or delete.
  - A lost race reloads and retries, and an outcome that can't be known deletes nothing, as Iceberg's commit does
    (spec 3).
  - Hooks and options are the ones the Iceberg crate has (`CommitHooks`, `CommitOptions`), made format-neutral.
- `expire(older_than, keep_last)`: removes snapshots and returns the files nothing refers to any more. It never
  removes branches or tags.

**What stays format-specific.** Anything one format has and others don't is reached through the format's own type
(`as_iceberg()`). Examples: Iceberg's branches and tags, Delta's checkpoints, and catalog commits (DuckLake, Iceberg
REST). The view doesn't pretend formats are the same where they aren't.

**Formats behind the view,** each a feature of `lakekit-table`:

| Format | Time partitions | Commit rank | Atomic commit |
|---|---|---|---|
| Iceberg (first) | `day(time)` and the other transforms | Data sequence number | `v<N>.metadata.json` if absent |
| Delta Lake | A generated column, identity-partitioned | The log version that added the file | `_delta_log/<N>.json` if absent |
| DuckLake | Its partition transforms | The snapshot that added the file | A transaction in its catalog database |

Delta and DuckLake each get a spec before they are built (spec 2).

**Send.** Native applications run commits on multi-threaded executors (Timelake's engine under tokio and
DataFusion), so the view needs `Send` futures where the store's are. That waits on
[loko#2](https://github.com/polythings/loko/issues/2).

## Decisions

| ID | Decision | Status |
|---|---|---|
| D18 | The common table view is its own crate, `lakekit-table`, above the format crates, with a feature per format; what only some formats have is reached through each format's own type | Accepted |
| D19 | A file's commit rank is the view's notion of write order (Iceberg's data sequence number, Delta's log version), so applications order duplicate rows the same way in every format | Proposed |

## Open questions and issues

- [#16](https://github.com/polythings/lakekit/issues/16) Statistics values in the common view: an enum of our own, or
  Arrow scalars?
- [#17](https://github.com/polythings/lakekit/issues/17) Partitioning in the common view: Iceberg's transforms, or
  source columns plus expressions?
- [#13](https://github.com/polythings/lakekit/issues/13) Build `lakekit-table` with Iceberg, and move Timelake's
  engine onto it
- [loko#2](https://github.com/polythings/loko/issues/2) `Send` futures for multi-threaded executors

## History

- 2026-10-21: proposed, so that Timelake's engine works on more than one table format and lakeglass shares the code.
