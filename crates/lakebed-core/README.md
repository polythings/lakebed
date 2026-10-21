# lakebed-core

The I/O boundary of [lakebed](https://github.com/polythings/lakebed)'s table format crates. Format crates do no I/O of
their own: they call a `Host` that the application implements over its object store, clock, and randomness, so they
build for `wasm32-unknown-unknown` and native targets alike.

- `Host`: get, ranged get, exists, put, put-if-absent (the commit primitive), multipart upload, list, delete, the time,
  random numbers, and waiting. Its futures aren't `Send`, so a host can wrap JavaScript promises.
- `MemoryHost`: a host in memory with a fixed clock and seeded randomness, for tests.
- `Error`: one error type, whose variant says what a caller can do (`NotFound`, `Conflict`, `CommitUnknown`, …).

Apache License 2.0.
