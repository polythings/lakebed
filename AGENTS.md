# Agent instructions

lakekit: table formats (Apache Iceberg first) read and written in Rust without I/O of their own, for WebAssembly and
native targets. One crate per format (`crates/lakekit-<format>`) on a shared I/O boundary (`crates/lakekit-core`).
Timelake and lakeglass use it.

## Before changing code

- Follow [CONTRIBUTING.md](CONTRIBUTING.md). It is the code guide, and it applies to you.
- For anything beyond a small fix, check the specs ([spec/README.md](spec/README.md)). Agree on the design with the user
  before building something new, and record the decision in the spec's Decisions table.

## Rules

- **Done means checked.** Before you report work as finished, run `prek run --all-files --hook-stage pre-push` and
  `cargo test --workspace --all-features`. Report failures as failures.
- **Fix the code, not the checks.** Don't loosen lint or format configs. Don't add `#[allow]`. Use
  `#[expect(..., reason = "...")]` only when the tool is genuinely wrong.
- **Keep things in sync:** a change in behaviour updates its spec (status, design, history), the tests, and
  `CHANGELOG.md` (under `Unreleased`) in the same commit.
- **Everything builds for WASM.** Every crate builds for `wasm32-unknown-unknown`: no threads, no blocking I/O, no
  `std::time`, no C dependencies. All I/O goes through the `Store` and `Env` traits. CI checks it.
- **Readers don't trust files.** Bytes from a store can be anything: check lengths, return errors, never panic.
- **Interop is tested.** What a format crate writes must read in that format's reference implementation and the other
  way round; the tests show it.
- **Every behaviour a caller might want to change is an option**, named for its mechanism.
- **Ideas, not code.** Never copy or translate another project's code. Credit ideas in `PRIOR_ART.md`
  (CONTRIBUTING.md, "Credit").
- **Tests run offline.** No network, cloud accounts, or real buckets in tests.
- **Never commit secrets** or `.env*` files.
