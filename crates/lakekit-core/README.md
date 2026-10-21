# lakekit-core

The I/O boundary of [lakekit](https://github.com/polythings/lakekit)'s table format crates. Format crates do no I/O of
their own: they read and write through a `Store`, take the time from a `Clock`, and take random numbers from a
`Random`, all supplied by the application, so they build for `wasm32-unknown-unknown` and native targets alike.

- `Store`: get, ranged get, exists, put, put-if-absent (the commit primitive), multipart upload, list, delete. Its
  futures aren't `Send`, so a store can wrap JavaScript promises. `lakekit-object-store` implements it over
  `object_store`.
- `Clock`: the time and a timer. `TokioClock` (feature `tokio`, native only) uses the system clock and tokio's timer.
- `Random`: random numbers. `OsRandom` (feature `getrandom`, native only) uses the operating system's generator.
- `Services`: a store, a clock, and a random source as one value, for APIs that need all three (commits).
- `MemoryStore`: all three in memory, with a clock that moves only when told and seeded randomness, for tests.
- `uuid_v4`, `uuid_v7`: identifiers from a `Random` (and a `Clock`), with the `uuid` crate.
- `Error`: one error type, whose variant says what a caller can do (`NotFound`, `Conflict`, `CommitUnknown`, …).

Apache License 2.0.
