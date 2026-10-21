---
number: 2
title: One crate per format
status: in-progress
updated: 2026-10-21
versions: [unreleased]
related: [1, 3, 4, 5, 6]
issues: [6, 11]
---

# 2. One crate per format

**In progress** · updated 2026-10-21 · versions unreleased · milestone: [0.1](milestones/v0-1.md)

## Summary

Each table format is its own crate, `lakekit-<format>`, depending on `lakekit-core` and nothing of another format's.
The `lakekit` crate re-exports them behind a feature each. Iceberg is first (spec 3), because Timelake writes it and
lakeglass reads it; the formats lakeglass explores come next.

## Design

- **Layout:** `crates/lakekit-core` (spec 1), `crates/lakekit-<format>`, and `crates/lakekit`, whose features are the
  format names (`iceberg`) and their parts (`iceberg-write`, `iceberg-arrow`, `iceberg-parquet-stats`).
- **Formats as they are.** A format crate exposes its format's own model (Iceberg's snapshots and manifests, Delta's
  log, Hudi's timeline) without a shared abstraction that would hide the differences. A common view, where an
  application needs one (lakeglass's table explorer), is built on top of the format crates, not inside them.
- **Reading first.** A format crate reads every version of its format that is in use, and writes what an application
  needs, starting with the version most readers support.
- **Interop is tested** against the format's reference implementation: it reads what we write, and we read what it
  writes, offline.
- **Building for WebAssembly:** every crate builds for `wasm32-unknown-unknown` and is checked for `wasm32-wasip2`.
- **Next formats,** in the order lakeglass needs them: Delta Lake, then Apache Hudi, Apache Paimon, Lance, and
  DuckLake. Each is proposed as its own spec before it is built.
- **Engines** get adapters the same way, a crate each, if they are built: a DataFusion table provider per format is
  an open question ([#11](https://github.com/polythings/lakekit/issues/11)).

## Decisions

| ID | Decision | Status |
|---|---|---|
| D4 | One repository with a crate per format and an umbrella crate with a feature each, not one crate with every format | Accepted |
| D5 | Format crates show their format as it is; a common view lives above them | Accepted |
| D6 | Arrow and Parquet follow the major version iceberg-rust's newest release uses, so applications using both build one copy | Superseded by D17 |
| D17 | Arrow and Parquet follow the major version of the newest DataFusion release, so applications can use DataFusion's newest features (Timelake's engine stops using iceberg-rust, spec 5); iceberg-rust stays in interop tests, which exchange files, not Arrow values | Accepted |

## Open questions and issues

- [#6](https://github.com/polythings/lakekit/issues/6) Which format comes after Iceberg, and in what order?
- [#11](https://github.com/polythings/lakekit/issues/11) DataFusion table providers (lakekit-datafusion)?

## History

- 2026-10-20: proposed and the layout built, with Iceberg as the first format.
- 2026-10-21: the project renamed from lakebed to lakekit, an easier name to read; crates `lakekit-core`,
  `lakekit-iceberg`, and `lakekit`.
- 2026-10-21: the common table view (spec 5) and data file formats (spec 6) proposed above the format crates; Arrow
  follows the newest DataFusion release instead of iceberg-rust (D17).
