# Specs

lakekit's design, laid out like Timelake's and lakeglass's. Start here, people and agents alike.

- **Specs** (`NNNN-name.md`): one feature each, with the same template ([TEMPLATE.md](TEMPLATE.md)). A spec says what
  the feature does and why, what is still open (as GitHub issues), how far it is built, and the releases that changed
  it.
- **Milestones** ([`milestones/`](milestones/README.md)): goals made of specs. The roadmap is the list of milestones.

## Statuses

| Status | Meaning |
|---|---|
| Done | Built and in effect; the spec describes how it works now. |
| In progress | Partly built: the spec says which parts are, and the open parts have issues. |
| Planned | Agreed or proposed, not built yet. |
| Dropped | Discussed and not done; kept so the reasons stay on record. |

## Writing and changing a spec

- **When:** anything beyond a small fix gets a spec, or a change to one, agreed before it is built.
- **New spec:** copy [TEMPLATE.md](TEMPLATE.md) to `NNNN-short-name.md` with the next free number (numbers are never
  reused), and add it to a milestone and to the index below.
- **Front matter:** `number`, `title`, `status` (`done`, `in-progress`, `planned`, `dropped`), `updated`, `versions`
  (the releases that changed it; `unreleased` until a release names it), and optionally `related` and `issues`.
- **Open questions** are GitHub issues (label `question`), listed under `issues`, not paragraphs in the spec.
- **Same commit:** when work lands, its spec's status, versions, and history change with the code and the changelog.

## Index

6 specs: 2 in progress, 2 done, 2 planned. Decisions D1–D21.

| # | Spec | Status | Updated |
|---:|---|---|---|
| 1 | [Store, Clock, and Random](0001-store-clock-random.md) | Done | 2026-10-21 |
| 2 | [One crate per format](0002-formats.md) | In progress | 2026-10-21 |
| 3 | [Apache Iceberg](0003-iceberg.md) | In progress | 2026-10-20 |
| 4 | [Store adapters](0004-store-adapters.md) | Done | 2026-10-21 |
| 5 | [A common table view](0005-table-view.md) | Planned | 2026-10-21 |
| 6 | [Data file formats](0006-file-formats.md) | Planned | 2026-10-21 |

## Milestones

| Milestone | Status |
|---|---|
| [0.1: Iceberg for Timelake and lakeglass](milestones/v0-1.md) | In progress |
| [0.2: Any table format, any file format](milestones/v0-2.md) | Planned |
