# PURGATORY

Custom native two-dimensional massively multiplayer online role-playing game (2D MMORPG), written in Rust. Dedicated server authority, native desktop client, shared authored content, and PostgreSQL durable storage.

**[Project documentation → GitHub Wiki](https://github.com/Arieltetenboim/Purgatory/wiki/Home)**

| Baseline | Value |
|---|---|
| Release label (`VERSION`) | `0.12.A` |
| Execution marker (`PHASE`) | `12.12C` |
| Network protocol (`PROTOCOL_VERSION`) | `34` |

Phase 12A–12C are accepted; the separate Phase 12 exit remains open. [Current State](https://github.com/Arieltetenboim/Purgatory/wiki/Current-State) owns the capability overview. [Phase 12](https://github.com/Arieltetenboim/Purgatory/wiki/Phase-12) owns acceptance and remaining exit requirements.

## Run

Windows is the primary development host. Install stable Rust, rustfmt, and Clippy as specified in [`rust-toolchain.toml`](rust-toolchain.toml).

```text
DEV_HUB.BAT
```

The Developer Hub is the normal entry point for server/client lifecycle, validation, logs, and standalone Labs. `DEV.BAT` is the PowerShell fallback. Follow [PostgreSQL Operations](https://github.com/Arieltetenboim/Purgatory/wiki/PostgreSQL-Operations) to create the development database and provision a username first. Normal server startup opens an initialized database; it does not create or migrate it.

Direct entry points:

```text
cargo run -p purgatory-server
cargo run -p purgatory-client
cargo run -p purgatory-map-lab
```

The local server defaults to `127.0.0.1:5001`. See [Developer Tools](https://github.com/Arieltetenboim/Purgatory/wiki/Developer-Tools) and [Content & Authoring](https://github.com/Arieltetenboim/Purgatory/wiki/Content-and-Authoring).

## Build and verify

```text
cargo build --workspace
```

Canonical quality gate: `./scripts/check.ps1` on Windows or `./scripts/check.sh` on Linux/macOS. [Testing & Quality](https://github.com/Arieltetenboim/Purgatory/wiki/Testing-and-Quality) explains separate PostgreSQL integration suites and manual proof.

## Repository

`apps/` contains the server, client, and Hub. `crates/` contains shared runtime libraries. `tools/` contains Labs, authoring utilities, and the load harness. `content/` contains authored definitions and checked runtime projections; `Graphic/` contains deliberately integrated artwork.

`master` is canonical. `DEVELOPMENT` is a deliberate stable checkpoint, not a parallel development line. Root `VERSION` is independent of Cargo package versions. Historical documentation is explicitly marked in [the Wiki archive](https://github.com/Arieltetenboim/Purgatory/wiki/Historical-Archive).
