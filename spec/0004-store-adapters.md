---
number: 4
title: Store adapters
status: in-progress
updated: 2026-10-21
versions: [unreleased]
related: [1]
issues: [10]
---

# 4. Store adapters

**In progress** · updated 2026-10-21 · versions unreleased · milestone: [0.1](milestones/v0-1.md)

## Summary

Ready-made `Store`s ([spec 1](0001-store-clock-random.md)) over the store libraries applications already use, so a
native application doesn't write its own. Each adapter is a crate of its own; the `Store` trait stays the boundary,
because some stores aren't these libraries (a Cloudflare R2 binding reached from JavaScript) and applications wrap
their store (lakeglass counts every request).

## Design

**`lakekit-object-store`** (built): `ObjectStoreAdapter` over an `Arc<dyn object_store::ObjectStore>` (0.14).

- **Names:** lakekit uses absolute URIs; the adapter has a root (`s3://bucket/`, `memory://`, `file:///` for a local
  file system) and maps `<root><path>` to the store's path `<path>`. A URI outside the root is `Error::Invalid`.
- **Operations:** `get` (missing is `None`), `get_range`, `exists` (a `head`), `put`, `put_if_absent` as
  `PutMode::Create` (`AlreadyExists` or `Precondition` is `false`), multipart uploads, `delete` (missing is fine,
  deletes run concurrently). `list` is by string prefix: the adapter lists the prefix's directory and keeps what
  starts with the prefix, since `object_store` lists whole path segments.
- **Construction:** `new(store, root)`, or `from_url(url, options)` through `object_store::parse_url_opts` for the
  URL's scheme and bucket.
- **Backends are the application's:** the crate enables no `object_store` features. Applications turn on `aws`,
  `gcp`, `azure`, `http`, or `fs` (or the `*-base` variants, which build for WebAssembly) on their own dependency, so
  the adapter builds for `wasm32-unknown-unknown` and pulls in nothing unasked.
- **Tests:** objects, conditional writes, multipart, prefix listing, and local files through the adapter, and an
  Iceberg commit loaded back through it.

**OpenDAL** ([#10](https://github.com/polythings/lakekit/issues/10)): the same over an OpenDAL `Operator`, once an
application needs it.

## Decisions

| ID | Decision | Status |
|---|---|---|
| D11 | Store libraries are adapters, one crate each, over the `Store` trait; lakekit's crates never depend on them | Accepted |
| D12 | An adapter enables none of its library's backends; the application chooses them | Accepted |

## Open questions and issues

- [#10](https://github.com/polythings/lakekit/issues/10) Store adapter for OpenDAL (lakekit-opendal)

## History

- 2026-10-21: proposed, and `lakekit-object-store` built.
