# Developer Tools roadmap

This is tooling evolution, not a PURGATORY gameplay phase. Phase 6G is the current game-runtime marker. Do not start Phase 7 from this work.

## CURRENT — foundation (this pass)

- Documentation under `docs/dev-tools/`
- Modular `tools/dev/` shell
- Direct process ownership (no WT/.cmd for server, client, or cargo)
- Owned cargo builds
- Server state machine and verified Ready
- `purgatory-load --probe` (protocol v10, persist-path debt documented)
- Runtime / Testing / Diagnostics UI sections
- Load testing, quality gate, reports preserved

## Next Developer Tools steps (not this pass)

Recommended order after this foundation is stable in daily use:

1. Tighten probe/health so Ready does not mint `dev.probe` character state (needs a dedicated design; may or may not need a protocol change — ask before any wire change).
2. Settings page for persist dir, listen/metrics ports, autostart, leave-running policy.
3. Richer diagnostics: follow `logs/dev-tools/server.log`, last probe stderr, tick/overrun from metrics without treating them as Ready.
4. Content inspection (read-only registry/map/entity JSON) before any editor.
5. Native visual Map Editor as a separate binary, launched from this shell if needed.

## Not started (explicit non-goals until requested)

- Map Editor, NPC Editor, dialogue/item editors
- Gameplay administration / combat tools
- Plugin framework
- Rewrite of Developer Tools in Rust
- Web-based Developer Tools
- Production/admin server tooling
