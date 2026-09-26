# Developer Tools roadmap

This tooling evolution is not a PURGATORY gameplay or capacity phase. Root `PHASE` is independent of Developer Tools work and is currently `11.closeout`. Animation Lab, NPC Lab, and Mob Lab are standalone authoring tools launched from the Hub; tooling work must not silently advance the main gameplay phase.

## CURRENT — PowerShell foundation + Hub launcher parity

- Documentation under `docs/dev-tools/` including [`PARITY.md`](PARITY.md)
- Modular `tools/dev/` shell (operational fallback; `DEV.BAT`)
- Rust Developer Hub: `purgatory-dev-runtime` + `purgatory-dev-hub` (`DEV_HUB.BAT` launches the exe independently)
- Server state machine, spawned vs adopted vs discovered, explicit jobs, bounded UI logs + file tails
- Hub lifetime ≠ dedicated server / client lifetime; reopen adopts + re-verifies
- Live pages: Dashboard, Runtime → Server / Clients, Validation, Performance, Logs, Settings, Content (Animation Lab, NPC Lab, and Mob Lab launch + authoring-template exports)
- FORGE W1.3A Map Lab visual foundation: standalone Rust/eframe preview over the shared canonical Tiled compiler; Hub launch/polish is deferred
- Runtime Validation (`--preset`) and load/soak (no `--preset`); CLI owns pass/fail; workspace `hub.lock`
- Clients +1/+2/+3, quality gate, Rebuild, Kill All, F5/F6, debug/release + log level for new processes
- `purgatory-load --probe` on the current protocol contract (DEV login/persist path originated in v10; persist-path debt documented)
- PowerShell remains fallback until an explicit decision switches `DEV.BAT`

## Next Developer Tools steps

1. Tighten probe/health so Ready does not mint `dev.probe` character state (needs a dedicated design; may or may not need a protocol change — ask before any wire change).
2. Finish Map Lab authoring-tool ergonomics and Hub launch only after W1.3A visual/compiler parity is owner-accepted; do not expand into FOOTNOTE/NPC/Mob editing from this slice.
3. Further content inspection remains incremental; Animation Lab A7.2 stays on its own track.
4. Switch `DEV.BAT` off only after an explicit owner decision that Hub parity is sufficient.

## Not started (explicit non-goals until requested)

- Map Lab gameplay editors (FOOTNOTE/NPC/Mob/portal/trigger/spawn) and broader dialogue/item editors beyond the existing NPC/Mob authoring tools
- Production/admin server tooling. DEV-authoritative Server Commands already exist for selected development operations (including authored NPC/Monster/item spawning and narrative controls); do not treat them as a production admin console.
- Plugin framework
- Web-based Developer Tools
- Production/admin server tooling
- Switching `DEV.BAT` off until Hub parity is sufficient
