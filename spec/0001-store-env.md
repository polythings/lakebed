---
number: 1
title: Store and environment
status: done
updated: 2026-10-21
versions: [unreleased]
related: [2, 3, 4]
issues: []
---

# 1. Store and environment

**Done** · updated 2026-10-21 · versions unreleased · milestone: [0.1](milestones/v0-1.md)

## Summary

Format crates do no I/O of their own. They read and write objects through a `Store` and take the time and randomness
from an `Env` (`lakekit-core`), both supplied by the application, so the same code runs in a browser, a Cloudflare
Worker, a CLI, or a server, and tests run against memory.

## Design

- **`Store`:** `get` (the object or `None`), `get_range`, `exists`, `put`, `put_if_absent` (the commit primitive:
  `false` when an object is there), `upload` (a multipart upload: parts in order, then complete or abort), `list` (by
  string prefix, not only by directory), and `delete` (missing objects are not an error). Paths are absolute URIs, as
  table metadata records them. Adapters implement it over store libraries ([spec 4](0004-store-adapters.md)).
- **`Env`:** `now_ms`, `random`, and `sleep`: what a writer needs besides its store (snapshot ids, file names, commit
  timestamps, the wait between commit attempts). Readers need only a `Store`.
- **`WithEnv`:** a store and an environment as one value (`WithEnv::new(store, env)`), for APIs that need both, such as
  `lakekit_iceberg::commit`. A JavaScript host that is both implements both traits on one type.
- **`TokioEnv`** (feature `tokio`, native targets only): the system clock, the operating system's randomness, and
  tokio's timer.
- **Futures aren't `Send`.** Methods return `impl Future`, without a `Send` bound, so a store can wrap JavaScript
  promises (wasm-bindgen). Native stores are free to be `Send`.
- **`MemoryStore`:** a store in memory that is also an environment, with a clock that only moves when told (or when
  `sleep` is called) and a seeded random sequence, so tests are deterministic.
- **`Error`:** one error type for all crates, with the variant saying what a caller can do: `Invalid`, `Unsupported`,
  `Conflict` (retry on the newer version), `CommitUnknown` (the commit may have landed: delete nothing it wrote, load
  the table again), `NotFound`, `Store`, and `Corrupt`. It is `non_exhaustive`.
- **Identifiers:** `uuid_v4` and `uuid_v7`, built by the `uuid` crate from an environment's randomness and clock (its
  builder needs neither its own randomness nor `std::time`, so it works in WebAssembly).

## Decisions

| ID | Decision | Status |
|---|---|---|
| D1 | Format crates do no I/O: everything goes through traits the application implements (the idea of delta-kernel-rs's `Engine`) | Accepted |
| D2 | Store and environment futures aren't `Send`, so JavaScript hosts work; native applications lose nothing | Accepted |
| D3 | One error type across crates, whose variants say what a caller can do | Accepted |
| D10 | Two small traits, `Store` for objects and `Env` for the clock, randomness, and waiting, rather than one `Host`: names that say what they are, readers that need only a store, and store adapters that don't decide how time is told | Accepted |

## Open questions and issues

None open.

## History

- 2026-10-20: built, from Timelake's `timelake_format::host`.
- 2026-10-21: one `Host` trait split into `Store` and `Env` (D10); `TokioEnv` and `WithEnv` added; UUIDs from the
  `uuid` crate. The spec was "The host boundary".
