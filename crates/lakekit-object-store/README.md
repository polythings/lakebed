# lakekit-object-store

A [lakekit](https://github.com/polythings/lakekit) `Store` over an [`object_store`](https://crates.io/crates/object_store)
client: S3, GCS, Azure, local files, memory, and anything else that implements `ObjectStore`.

```rust
use std::sync::Arc;

use lakekit_object_store::ObjectStoreAdapter;
use object_store::memory::InMemory;

let store = ObjectStoreAdapter::new(Arc::new(InMemory::new()), "memory://");
// Or from a URL, with the backend enabled on `object_store` (here `aws`):
// let store = ObjectStoreAdapter::from_url("s3://bucket/warehouse", [("aws_region", "us-east-2")])?;
```

- lakekit names objects by absolute URI; the adapter maps them to paths under its root (`s3://bucket/`, or
  `file:///` for local files).
- Put-if-absent, the commit primitive, is `PutMode::Create`; another writer's object reads as `false`.
- Listing is by string prefix (lakekit's `…/metadata/v`), done by listing the prefix's directory.
- The crate enables no `object_store` backends: turn on the ones you need (`aws`, `gcp`, `azure`, `http`, `fs`, or the
  `*-base` variants for WebAssembly) on your own `object_store` dependency.

Apache License 2.0.
