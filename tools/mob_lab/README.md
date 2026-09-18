# Mob Lab

Status: FORGE M3 v0.1 editor foundation.

Mob Lab edits the **same Monster JSON files loaded by the game**:

```text
tools/mob_lab
  -> content/definitions/monsters/*.json
  -> purgatory-content
  -> server runtime
```

There is no authoring/export shadow copy.

## Run

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\mob_lab\run.ps1
```

Default URL: `http://127.0.0.1:8766/`.

## M3 scope

- list runtime Monster definitions;
- create a schema-v3 Monster for an already allocated stable ContentId;
- edit identity/body/movement/behavior;
- preview the actual runtime sprite with asymmetric Left/Right/Bottom/Top collision bounds in world-unit scale;
- inspect/apply raw JSON;
- local shape validation;
- atomic save directly to `content/definitions/monsters`;
- canonical Rust runtime-pack validation on every save;
- automatic rollback if runtime validation fails.

Test Arena/runtime spawning belongs to M4. Developer Hub integration belongs to M5.

## Save safety

Mob Lab performs a cheap local shape check first. On Save it atomically writes the candidate, runs:

```powershell
cargo run -q -p purgatory-content-validator
```

If the real content pack fails, Mob Lab restores the previous file (or removes a failed newly-created file).

## Tests

```powershell
py -3 -m unittest discover -s .\tools\mob_lab -p "test_*.py"
```

## ContentId boundary

M3 does **not** allocate new stable numeric ContentIds. That belongs to the
shared migration/allocation work tracked by issue #24.

Mob Lab reads the checked catalog and shows the numeric ContentId for existing
Monster labels. NEW MONSTER refuses an unallocated label with an explicit
message instead of silently creating a second identity scheme or editing Rust
catalog source.
