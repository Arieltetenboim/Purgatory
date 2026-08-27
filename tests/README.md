# Tests

Phase 0 tests live beside each crate and binary:

- `crates/*/src/lib.rs` unit tests
- `apps/*/src/main.rs` linkage tests
- `tools/*/src/main.rs` linkage tests

Workspace command:

```text
cargo test --workspace
```

Integration and network handshake tests live beside `apps/server/src/network/tests.rs` (localhost Quinn). Protocol unit tests live in `crates/protocol`. Do not add prediction or load-test harnesses before those phases.
