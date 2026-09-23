# PURGATORY

Custom-built native 2D side-scrolling MMORPG in Rust.

Built from scratch around a server-authoritative simulation, native desktop client, QUIC networking, shared content, persistence, animation, and development tooling.

This README is intentionally small. It is the project entry point, not the project history.

## Current

- **Master version:** `0.11.F`
- **Phase:** `11.closeout` — Phase 11 Item Loop complete
- **FORGE M:** M4 implementation is merged to `master`; GitHub Issue #75 remains open for the recorded manual two-Monster visual smoke and final M4/M5 closeout evidence
- **FORGE N:** NPC authoring/runtime N10a-N10f complete and merged
- **Character Lab:** integrated on `master`; Hub launch, Humanoid v0 contract export, Template V1 (2048×2048) import/validation/conversion, and the current visual-pack/atlas path are present
- **Production client UI:** Inventory foundation, player-facing Settings, and the Glyphon production-text foundation are merged
- **Main gameplay next:** Phase 12 — Character Continuity, intentionally not started
- **Dash / learned ability:** authoritative Dash + NPC `GrantAbility` integration restored on `master`; Shift activates Dash after it has been granted
- **Protocol:** v31
- **Simulation:** server authoritative
- **Client:** native `winit` + `wgpu`
- **Networking:** QUIC via Quinn
- **Language:** Rust stable, edition 2024

The root [`PHASE`](PHASE) file is the exact gameplay-phase marker. Parallel FORGE/tooling work does not move it unless explicitly promoted into the main gameplay sequence.

## Run

### Normal development

```text
DEV_HUB.BAT
```

The Developer Hub is the normal development entry point for server/client lifecycle, validation, load tools, rebuilds, and standalone authoring-tool launch.

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
| `tools/Character part lab` | Character Lab authoring/conversion tool |
| `tools/mob_lab` | Monster authoring / creature-manifest tool |
| `tools/npc_lab` | NPC authoring/test tool |
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
| Left / Right Shift | Dash (when granted) |
| `~` | Toggle debug overlay |

## Branch & version policy

- `master` is the canonical current development/release branch.
- Root `VERSION` is the human-facing master version label (currently `0.11.F`).
- `DEVELOPMENT` is a deliberate stable checkpoint/rollback branch. It is **not** a parallel development line and is advanced only after selected stable master versions.
- Feature/fix/salvage branches are temporary. Once their useful work is integrated or explicitly superseded, delete them instead of keeping long-lived stale branches.
- Cargo package versioning remains valid Semantic Versioning (SemVer); the master label may use the project's `0.11.F` scheme independently.


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
| [`PHASE`](PHASE) | Exact current gameplay phase |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Development roadmap and active parallel tracks |
| [`docs/PURGATORY_PROJECT_CONTEXT.md`](docs/PURGATORY_PROJECT_CONTEXT.md) | Durable AI/development reasoning doctrine; never a substitute for current repo evidence |
| [`docs/PROJECT_ENGINEERING_NOTES.md`](docs/PROJECT_ENGINEERING_NOTES.md) | Engineering context, manual observations, rejected hypotheses, tool constraints and deferred polish |
| [`docs/QUALITY.md`](docs/QUALITY.md) | Repository quality, ownership and refactoring policy |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | System architecture |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Architecture Decision Records (ADRs) and frozen decisions |
| [`docs/PROTOCOL.md`](docs/PROTOCOL.md) | Network protocol contract |
| [`docs/TEST_GATES.md`](docs/TEST_GATES.md) | Validation and test gates |
| [`docs/PERFORMANCE_BUDGETS.md`](docs/PERFORMANCE_BUDGETS.md) | Performance budgets |
| [`docs/CONTENT_PIPELINE.md`](docs/CONTENT_PIPELINE.md) | Content pipeline |
| [`docs/NPC_DIALOGUE_RUNTIME.md`](docs/NPC_DIALOGUE_RUNTIME.md) | Current NPC dialogue runtime, ownership and operation |
| [`docs/CHARACTER_ANIMATION_ARCHITECTURE.md`](docs/CHARACTER_ANIMATION_ARCHITECTURE.md) | Character animation architecture |
| [`docs/dev-tools/README.md`](docs/dev-tools/README.md) | Developer tooling details |

## Requirements

- Rust stable via `rustup` — see [`rust-toolchain.toml`](rust-toolchain.toml)
- `rustfmt`
- `clippy`
- Windows is the primary development host

---

**Rule of thumb:** if information is historical, phase-specific, architectural, or operationally detailed, it belongs in `docs/`, not in this README.
