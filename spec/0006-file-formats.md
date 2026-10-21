---
number: 6
title: Data file formats
status: planned
updated: 2026-10-21
versions: []
related: [2, 3, 5]
issues: [11, 14, 15]
---

# 6. Data file formats

**Planned** · updated 2026-10-21 · milestone: [0.2](milestones/v0-2.md)

## Summary

A table's data files are written and read through a `FileFormat` trait (`lakekit-file`), one crate per format: Parquet
first (`lakekit-parquet`), Vortex once Iceberg lists it. Applications don't assume Parquet, and a table can change
format over time, file by file. The crates live in lakekit for now and can move to a library of their own later.

## Design

**`FileFormat`,** with no I/O of its own, like the table formats (spec 1):

- **Name:** the name the table format records for a file (Iceberg's `file_format`: `PARQUET`, later `VORTEX`).
- **Writing:**
  - A writer takes Arrow batches and returns encoded bytes as it goes, for the caller to upload.
  - At the end it returns the file's statistics: row count, and per field id min, max and null count, as the common
    view holds them (spec 5).
  - It also returns the footer: the bytes a reader needs before reading data (Parquet's metadata and page index).
    Applications can keep the footer apart, as Timelake's head does, so a cold read skips fetching it.
- **Statistics of a file someone else wrote,** from its footer. Iceberg's statistics come from here; this replaces
  `lakekit-iceberg`'s `parquet-stats` feature.
- **Reading:**
  - A reader takes the footer, the columns, and a selection inside the file (Parquet's row groups and pages), and
    returns the byte ranges it needs.
  - Given those bytes, it returns Arrow batches in the file's row order.
  - The application fetches the bytes, so browsers and native applications read the same way.
- **Options:** each format's write options (Parquet's compression, row-group rows, encodings), parsed from strings,
  so a table property can choose them.

**Formats:**

- **Parquet** (first): today's Parquet code from Timelake's `timelake-format` (writer, footers, statistics) and
  `lakekit-iceberg` (statistics), made general.
- **Vortex**, once Iceberg's spec lists it ([apache/iceberg#15915](https://github.com/apache/iceberg/pull/15915),
  [apache/iceberg-rust#2835](https://github.com/apache/iceberg-rust/pull/2835)). Vortex's crates build for
  `wasm32-unknown-unknown`. Until Iceberg lists it, a table with Vortex files is readable only by engines that know
  Vortex, so an application offers it as an option that is off by default, with that said in its docs.

**Scans in DataFusion** are not in these crates. A `FileFormat` gives DataFusion's `FileSource` what it needs (the
footer, the selection), and the providers are [#11](https://github.com/polythings/lakekit/issues/11)'s
`lakekit-datafusion`.

## Decisions

| ID | Decision | Status |
|---|---|---|
| D20 | Data file formats are a `FileFormat` trait in `lakekit-file` and a crate per format (`lakekit-parquet` first), in lakekit for now; they can move to a library of their own later | Accepted |
| D21 | File formats do no I/O either: writers return bytes, and readers ask for byte ranges and decode what they're given | Proposed |

## Open questions and issues

- [#11](https://github.com/polythings/lakekit/issues/11) DataFusion table providers (`lakekit-datafusion`)
- [#14](https://github.com/polythings/lakekit/issues/14) Build `lakekit-file` and `lakekit-parquet` from Timelake's
  Parquet code
- [#15](https://github.com/polythings/lakekit/issues/15) Vortex data files, once Iceberg lists the format

## History

- 2026-10-21: proposed, so that applications don't assume Parquet and can use Vortex once Iceberg supports it.
