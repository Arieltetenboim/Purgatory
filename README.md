# PURGATORY

Custom-built native 2D side-scrolling MMORPG in Rust.

Built from scratch around a server-authoritative simulation, native desktop client, QUIC networking, shared content, persistence, animation, and development tooling.

This README is intentionally small. It is the project entry point, not the project history.

## Current

- **Phase:** `11.closeout` — Phase 11 Item Loop complete
- **Next:** Phase 12 — Character Continuity
- **Protocol:** v24
- **Simulation:** server authoritative
- **Client:** native `winit` + `wgpu`
- **Networking:** QUIC via Quinn
- **Language:** Rust stable, edition 2024

The root [`PHASE`](PHASE) file is the exact current phase marker.

## Run

### Normal development

```text
DEV_HUB.BAT
```

The Developer Hub is the normal development entry point for server/client lifecycle, validation, load tools, rebuilds, and Animation Lab.

Fallback developer shell:

```text
DEV.BAT
```

### Direct client

```powershell
cargo run -p purgatory-client
```

### Direct server

```powershell
cargo run -p purgatory-server
```

The server listens on `127.0.0.1:5001` by default.

## Project map

| Path | Role |
|---|---|
| `apps/client` | Native game client |
| `apps/server` | Headless authoritative server |
| `apps/dev_hub` | Developer Hub |
| `crates/common` | Shared primitives |
| `crates/simulation` | Authoritative gameplay simulation |
| `crates/protocol` | Client/server protocol |
| `crates/content` | Content definitions |
| `crates/persistence` | Character persistence |
| `crates/skeleton` | Character skeleton and pose math |
| `crates/animation` | Animation runtime |
| `crates/dev_runtime` | Developer Hub orchestration |
| `tools/animation_lab` | Animation authoring/debug tool |
| `tools/bot_client` | Headless load-testing client |
| `tools/content_validator` | Content validation |

## Development notes

- Default client builds include development diagnostics and the in-window debug overlay.
- **Backquote / `~`** toggles the debug overlay.
- The client starts on the Connection Frontend and does not auto-connect in normal development builds.
- Default local server address: `127.0.0.1:5001`.
- File-backed character data lives outside the repository. On Windows the default is under `%LOCALAPPDATA%\Purgatory\` unless overridden.
- `Graphic/` is not a general runtime asset scan. Visual assets are integrated deliberately through the relevant runtime paths.

### Core controls

| Input | Action |
|---|---|
| `A` / Left | Move left |
| `D` / Right | Move right |
| `S` / Down | Hold down |
| Space | Jump |
| Down + Jump | Drop through OneWay platform |
| `E` | Generic interact |
| Up Arrow | Activate portal |
| `J` | Basic Strike |
| `~` | Toggle debug overlay |

## Quality

Main local quality gate:

```powershell
./scripts/check.ps1
```

Linux / macOS:

```bash
./scripts/check.sh
```

The detailed validation policy and extended network/load gates live in the docs rather than here.

## Important docs

| Document | Purpose |
|---|---|
| [`PHASE`](PHASE) | Exact current development phase |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Development roadmap and phase history |
| [`docs/PROJECT_ENGINEERING_NOTES.md`](docs/PROJECT_ENGINEERING_NOTES.md) | Engineering context, manual observations, rejected hypotheses, tool constraints and deferred polish |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | System architecture |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Architecture Decision Records (ADRs) and frozen decisions |
| [`docs/PROTOCOL.md`](docs/PROTOCOL.md) | Network protocol contract |
| [`docs/TEST_GATES.md`](docs/TEST_GATES.md) | Validation and test gates |
| [`docs/PERFORMANCE_BUDGETS.md`](docs/PERFORMANCE_BUDGETS.md) | Performance budgets |
| [`docs/CONTENT_PIPELINE.md`](docs/CONTENT_PIPELINE.md) | Content pipeline |
| [`docs/CHARACTER_ANIMATION_ARCHITECTURE.md`](docs/CHARACTER_ANIMATION_ARCHITECTURE.md) | Character animation architecture |
| [`docs/dev-tools/README.md`](docs/dev-tools/README.md) | Developer tooling details |

## Requirements

- Rust stable via `rustup` — see [`rust-toolchain.toml`](rust-toolchain.toml)
- `rustfmt`
- `clippy`
- Windows is the primary development host

---

**Rule of thumb:** if information is historical, phase-specific, architectural, or operationally detailed, it belongs in `docs/`, not in this README.
