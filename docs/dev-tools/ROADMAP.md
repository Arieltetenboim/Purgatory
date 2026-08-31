# Developer Tools roadmap

This is tooling evolution, not a PURGATORY gameplay phase. Phase 6G is the current game-runtime marker. Do not start Phase 7 from this work.

## CURRENT — PowerShell foundation + Hub Slice 1 + Slice 2

- Documentation under `docs/dev-tools/` including [`PARITY.md`](PARITY.md)
- Modular `tools/dev/` shell (operational fallback; `DEV.BAT`)
- Rust Developer Hub Slice 1 / 1.1 / 2: `purgatory-dev-runtime` + `purgatory-dev-hub` (`DEV_HUB.BAT` launches the exe independently)
- Server state machine, spawned vs adopted vs discovered, explicit jobs, bounded UI logs
- Hub lifetime ≠ dedicated server lifetime; reopen adopts + re-verifies
- Dashboard / Runtime → Server / Validation / Logs are live; other nav categories are placeholders
- Runtime Validation forwards `purgatory-load --preset`; CLI owns pass/fail; workspace `hub.lock`
- `purgatory-load --probe` (protocol v10, persist-path debt documented)
- PowerShell still owns clients, quality gate, load dialog, OPEN FOLDER/REPORT

## Next Developer Tools steps

1. Hub Slice 3 — load/soak launcher (6G.2 capacity surface).
2. Later parity from [`PARITY.md`](PARITY.md) (clients, quality gate, Kill All, settings).
3. Tighten probe/health so Ready does not mint `dev.probe` character state (needs a dedicated design; may or may not need a protocol change — ask before any wire change).
4. Content inspection (read-only) before any editor.
5. Native visual Map Editor later as a Hub module or launched binary.

## Not started (explicit non-goals until requested)

- Map Editor, NPC Editor, dialogue/item editors
- Gameplay administration / combat tools / admin protocol
- Plugin framework
- Web-based Developer Tools
- Production/admin server tooling
- Switching `DEV.BAT` off until Hub parity is sufficient
