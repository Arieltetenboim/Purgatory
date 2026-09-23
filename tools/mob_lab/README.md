# Mob Lab

Status: FORGE M3 authoring foundation and M4 runtime integration are merged to `master`. Automated M4 closeout is GREEN; GitHub Issue #75 remains open for the required manual two-Monster visual/runtime smoke before final M4/M5 closeout.

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

## M3 authoring scope — merged

- list runtime Monster definitions;
- create a schema-v4 Monster and atomically allocate the next permanent Monster ContentId;
- edit identity/body/movement/behavior;
- select the real runtime sprite discovered from `Graphic/creature/*/manifest.json`;
- preview the actual runtime sprite with asymmetric Left/Right/Bottom/Top collision bounds in world-unit scale;
- author creature manifest v1/v2 presentation data, including explicit v2 frames/origins, sockets, variable-duration animation steps, and annotations;
- inspect/apply raw JSON;
- local shape validation;
- atomic save directly to `content/definitions/monsters`;
- canonical Rust runtime-pack validation on every save;
- automatic rollback if runtime validation fails.

Developer Hub launch and DEV-authoritative Monster spawn-by-ContentId are merged. M4 per-entity identity/presentation, Red Slime de-specialization, and per-Monster approach/contact geometry are also merged and automated GREEN. Issue #75 remains open only for the required manual two-Monster visual/runtime acceptance evidence. Final FORGE M closeout remains M5.

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

Mob Lab allocates new Monster IDs only from the frozen Monster block
(`10001–19999`) and writes the existing checked catalog/ledger owned by issue #24.
It does not introduce a second identity scheme. NEW MONSTER updates the Rust catalog,
the checked ledger, and the runtime Monster JSON as one validated transaction; a
failed runtime validation rolls all of them back.

## Sprite selection

Monster JSON stores only a stable authored sprite id such as a `creature.*` manifest id.
Mob Lab discovers valid sprite manifests directly from `Graphic/creature/*/manifest.json`;
there is no hardcoded Monster-to-sprite table in the Lab.

The client consumes the same Monster `sprite` presentation projection and loads the
matching manifest/atlas at startup. Changing sprite therefore changes the real runtime
presentation after rebuilding/restarting the client.
