# lakekit-core

The I/O boundary of [lakekit](https://github.com/polythings/lakekit)'s table format crates. Format crates do no I/O of
their own: they read and write through a `Store` and take the time and randomness from an `Env`, both supplied by the
application, so they build for `wasm32-unknown-unknown` and native targets alike.

- `Store`: get, ranged get, exists, put, put-if-absent (the commit primitive), multipart upload, list, delete. Its
  futures aren't `Send`, so a store can wrap JavaScript promises. `lakekit-object-store` implements it over
  `object_store`.
- `Env`: the time, random numbers, and waiting. `TokioEnv` (feature `tokio`, native only) uses the system clock, the
  operating system's randomness, and tokio's timer.
- `WithEnv`: a store and an environment as one value, for APIs that need both (commits).
- `MemoryStore`: both in memory, with a clock that moves only when told and seeded randomness, for tests.
- `uuid_v4`, `uuid_v7`: identifiers from an environment's randomness and clock (with the `uuid` crate).
- `Error`: one error type, whose variant says what a caller can do (`NotFound`, `Conflict`, `CommitUnknown`, …).

Apache License 2.0.
