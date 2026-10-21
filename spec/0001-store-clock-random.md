---
number: 1
title: Store, Clock, and Random
status: done
updated: 2026-10-21
versions: [unreleased]
related: [2, 3, 4]
issues: []
---

# 1. Store, Clock, and Random

**Done** · updated 2026-10-21 · versions unreleased · milestone: [0.1](milestones/v0-1.md)

## Summary

Format crates do no I/O of their own. They read and write objects through a `Store`, take the time from a `Clock`, and
take random numbers from a `Random` (`lakekit-core`), all supplied by the application, so the same code runs in a
browser, a Cloudflare Worker, a CLI, or a server, and tests run against memory.

## Design

- **`Store`:** `get` (the object or `None`), `get_range`, `exists`, `put`, `put_if_absent` (the commit primitive:
  `false` when an object is there), `upload` (a multipart upload: parts in order, then complete or abort), `list` (by
  string prefix, not only by directory), and `delete` (missing objects are not an error). Paths are absolute URIs, as
  table metadata records them. Adapters implement it over store libraries ([spec 4](0004-store-adapters.md)).
- **`Clock`:** `now_ms` and `sleep`: commit timestamps, time-ordered file names, and the wait between commit attempts.
- **`Random`:** `next_u64`: snapshot ids, file names, and the jitter between commit attempts.
- **Who needs what:** readers need only a `Store`; commits need `Store + Clock + Random`.
- **`Services`:** a store, a clock, and a random source as one value (`Services::new(store, clock, random)`), for APIs
  that need all three. A type that is all three (a JavaScript host, `MemoryStore`) needs no wrapper.
- **Native implementations:** `TokioClock` (feature `tokio`: the system clock and tokio's timer) and `OsRandom`
  (feature `getrandom`: the operating system's generator), on native targets only.
- **Futures aren't `Send`.** Methods return `impl Future`, without a `Send` bound, so a store or clock can wrap
  JavaScript promises (wasm-bindgen). Native ones are free to be `Send`.
- **`MemoryStore`:** a store in memory that is also a clock that only moves when told (or when `sleep` is called) and a
  seeded random sequence, so tests are deterministic.
- **`Error`:** one error type for all crates, with the variant saying what a caller can do: `Invalid`, `Unsupported`,
  `Conflict` (retry on the newer version), `CommitUnknown` (the commit may have landed: delete nothing it wrote, load
  the table again), `NotFound`, `Store`, and `Corrupt`. It is `non_exhaustive`.
- **Identifiers:** `uuid_v4` (from a `Random`) and `uuid_v7` (from a `Clock + Random`), built by the `uuid` crate (its
  builder needs neither its own randomness nor `std::time`, so it works in WebAssembly).

## Decisions

| ID | Decision | Status |
|---|---|---|
| D1 | Format crates do no I/O: everything goes through traits the application implements (the idea of delta-kernel-rs's `Engine`) | Accepted |
| D2 | Store and clock futures aren't `Send`, so JavaScript hosts work; native applications lose nothing | Accepted |
| D3 | One error type across crates, whose variants say what a caller can do | Accepted |
| D10 | Objects are a `Store` of their own, apart from the time and randomness: readers need only a store, and store adapters don't decide how time is told | Accepted |
| D13 | The time and randomness are two traits, `Clock` and `Random`, as ZIO's services are, rather than one environment trait: each says what it is, and each can be swapped on its own | Accepted |

## Open questions and issues

None open.

## History

- 2026-10-20: built, from Timelake's `timelake_format::host`.
- 2026-10-21: one `Host` trait split into `Store` (D10), `Clock`, and `Random` (D13); `Services`, `TokioClock`, and
  `OsRandom` added; UUIDs from the `uuid` crate. The spec was "The host boundary".
