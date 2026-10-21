# lakekit-core

The I/O boundary of [lakekit](https://github.com/polythings/lakekit)'s table format crates. Format crates do no I/O of
their own: they read and write through a `Store`, take the time from a `Clock`, and take random numbers from a
`Random`, all supplied by the application ([loko](https://github.com/polythings/loko)'s resources, re-exported here), so they build for `wasm32-unknown-unknown` and native targets alike.

- `Store`: get, ranged get, exists, put, put-if-absent (the commit primitive), multipart upload, list, delete. Its
  futures aren't `Send`, so a store can wrap JavaScript promises. `loko-object-store` implements it over
  `object_store`.
- `Clock`: the time and a timer. `TokioClock` (feature `tokio`, native only) uses the system clock and tokio's timer;
  `TestClock` moves only when told.
- `Random`: random numbers. `OsRandom` (feature `getrandom`, native and WASI) uses the operating system's generator;
  `SeededRandom` repeats a sequence from a seed.
- `Resources`: a store, a clock, and a random source as one value, for APIs that need all three (commits).
- `MemoryResources`: a `MemoryStore`, a `TestClock`, and a `SeededRandom`, for tests.
- `uuid_v4`, `uuid_v7`: identifiers from a `Random` (and a `Clock`), with the `uuid` crate.
- `Error`: one error type, whose variant says what a caller can do (`NotFound`, `Conflict`, `CommitUnknown`, …); a
  store's error converts into it.

Apache License 2.0.
