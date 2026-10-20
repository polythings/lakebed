# Contributing to lakebed

This is the code guide for everyone who changes this repository, people and AI agents alike. Formatters and linters
enforce the mechanical rules (see [Checks](#checks)). This guide covers what they can't: how code should read.
[spec/](spec/README.md) holds the design.

## Principles

1. **Readable first.** Someone new should understand a function from its name, its signature, and one pass through
   the body.
2. **One crate per format.** A format's crate depends on `lakebed-core` and on nothing of another format's, so adding
   one never touches the others. Formats are shown as they are: no shared model flattens their differences.
3. **No I/O of our own.** Crates read and write through `lakebed_core::Host`. They never open sockets, files, or
   threads, so they build for `wasm32-unknown-unknown`.
4. **One definition of each thing.** Before writing a helper, constant, or type, look for an existing one.
5. **Behaviour, spec, and tests change together.** A change users can see updates its spec, adds or updates tests, and
   adds a line to [CHANGELOG.md](CHANGELOG.md).
6. **Behaviour is configurable.** Where callers could reasonably want something else (retries, limits, what to write),
   it is an option with a default, named for its mechanism (`merge_manifests_at`, not `fast`).

## Comments and docs

- **Public items get a short doc comment**: what it does, and anything a caller must know (errors, units,
  invariants). One to three lines is usually enough.
- **Comments explain why, not what.** Explain a non-obvious choice, a workaround, or a rule from a format's spec, with
  a link to the spec section.
- **No long comments.** If an explanation needs a page, it belongs in a spec. Link to it from the code.

## Credit

lakebed learns from other projects' ideas, never from copying their code. [PRIOR_ART.md](PRIOR_ART.md) lists the
ideas we took.

- **Never copy code**, whatever its license: not by pasting, and not by translating it into another language.
- **Credit ideas from open-source projects** with a row in PRIOR_ART.md and a short comment where the idea is
  implemented.
- **Don't name open-core, dual-licensed, or source-available projects, or commercial products, as a source.**

## Rust

- **Errors:** `lakebed_core::Error`, with the variant that says what a caller can do about it (`NotFound`, `Conflict`,
  `CommitUnknown`, `Unsupported`, `Corrupt`). No `unwrap()` outside tests.
- **Types over strings:** enums and newtypes instead of strings and magic numbers. Parse once at the boundary.
- **Don't trust files.** Check lengths and offsets before using them, cap allocations, and return an error instead of
  panicking on a malformed file. Tests feed readers truncated and garbage input.
- **Async:** host futures aren't `Send` (JavaScript promises aren't). Never block inside async code.
- **Suppressing a lint:** `#[expect(lint, reason = "...")]` with a real reason. Never `#[allow]`.

## Tests

- **A bug fix starts with a test that fails**, then the fix.
- **Tests run offline**, against `MemoryHost` or a host over the reference implementation's in-memory storage.
- **Interop:** writers are tested against the format's reference reader, and readers against the reference writer.

## Checks

Install once: `uv tool install prek && prek install --install-hooks`. Hooks then run on commit and on push. Before you
push, or before calling work done:

```bash
prek run --all-files --hook-stage pre-push   # rustfmt, clippy (native and wasm32)
cargo test --workspace --all-features        # unit, doc, and interop tests
```

## Dependencies

Keep dependencies on their newest versions; check the latest release when adding one. Arrow and Parquet follow the
major version iceberg-rust's newest release uses (the interop tests build both). Actions are pinned to a commit with
the version in a comment (`uses: actions/checkout@<sha> # v7.0.1`).

## Commits

- **Subject:** a short imperative sentence with the area first, for example `Core: …`, `Iceberg: …`, or `Spec: …`.
  Add a body when the why isn't obvious.
- **Scope:** one logical change per commit.

## Changelog

[CHANGELOG.md](CHANGELOG.md) is for users, not a list of commits. Anything a user can notice gets one line under
`## Unreleased`, in `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, or `Security`, in the same commit as the
change.

## License

Contributions are licensed under the Apache License 2.0, like the rest of the project (see [LICENSE](LICENSE)).
