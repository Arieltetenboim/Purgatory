# NPC Lab

Status: **FORGE N1 — Local Web Shell (local working slice).**

NPC Lab is a local Web authoring tool for PURGATORY NPC content.

## Ownership

```text
local NPC Lab UI
-> content/authoring/npcs/*.json
-> authoring validation
-> runtime projection later
```

Repository JSON remains the source of truth. NPC Lab has no private database and no hosted service.

## N1 implementation

N1 intentionally uses no Web framework or new package dependency:

- `server.py` — Python 3 standard-library local HTTP/API bridge.
- `web/` — static HTML/CSS/JavaScript UI.
- `run.ps1` — local launcher; reopens an existing N1 server when one is already running.
- Developer Tools button — opens NPC Lab through the existing local Developer Hub.

The server binds to `127.0.0.1` only and defaults to port `8765`.

## N1 capabilities

- list canonical NPC authoring JSON recursively;
- filter the list;
- open a document;
- edit canonical JSON directly;
- dirty-state tracking;
- save with atomic local file replacement;
- create a minimal new NPC document;
- basic N1 contract validation;
- inspector summary;
- Ctrl+S save.

The Identity / Dialogue / State / Behavior / Presentation / Test surfaces are visible only as future tabs. N1 does not implement them.

## Run

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\npc_lab\run.ps1
```

Or use the **NPC LAB** button in PURGATORY Developer Tools.

## Tests

```powershell
py -3 -m unittest .\tools\npc_lab\test_server.py
```

If `py` is unavailable but `python` resolves to Python 3:

```powershell
python -m unittest .\tools\npc_lab\test_server.py
```

## N1 gate

Create an NPC, save it, reopen it, and confirm the authored data is unchanged.

## Still out of scope

- runtime NPC loader;
- quest manager;
- JavaScript gameplay scripting;
- graph editor;
- real Dialogue authoring surface;
- Behavior Tree authoring;
- voice pipeline;
- localization;
- database/cloud service.

See:

- `docs/NPC_AUTHORING_CONTRACT.md`
- GitHub Issue #17 — `[FORGE / N] NPC Lab — local authoring tool roadmap`
