# Branch consolidation archive — 2026-10-03

Audited canonical master: `8b3ccd5ee6440062f98e599fb987dfb1f20d39db`.
All 21 remote branches were inspected. `master` is retained. Deletion must
recheck each branch head against the values below; a branch that advanced
after this audit is not covered. Remote deletion is a separate operation and
is not implied by this document or by closing a pull request.

## Already integrated branches

For every row below, GitHub's comparison and Git ancestry show zero unique
commits relative to audited master. Deleting the branch does not delete its
commits or change master. No new merge is necessary.

| Branch | Audited head |
|---|---|
| `docs/phase12c-acceptance-exit-audit` | `6935dc0cb7a591122571784d4a378a61dddc2a89` |
| `fix/canonical-map-definition-owner` | `8dab4b78d6a8727b8acc59164682534ffaf09a77` |
| `fix/issue-44-authoritative-reset-spawn` | `a11d3d81c78bca3b542b89b6201cd734d8a96035` |
| `fix/issue-107-focus-held-cancel` | `b3ddb877c14650454c466f0df52e5ff9fe05033f` |
| `fix/item-lab-existing-equipment-save` | `7a5bd11d97ccb0c291ee98cbaea7de34f79e29b9` |
| `forge/item-lab-v1` | `60699fdcda7d1dfd42253fd0816c9288c65143c5` |
| `forge/map-lab-new-map-v1` | `2bb2db2d23def40faefe6a543f8bc38173d78906` |
| `integration/map-stack-20260927` | `286939a78b38c23bdd73c84216abb9fb91475a3f` |
| `migrate/issue-24-ability-content-ids` | `0ae0911d9c17fc8a9cccfed79bfafb42f23680e7` |
| `migrate/issue-24-world-object-portals` | `6d333079e6c0cd2d639337b6899daa52708750cc` |
| `phase12/issue-10-persistence-design` | `7aac3b358511d67372abc63d40670e3e09da75ac` |
| `phase12/12a-postgresql-foundation` | `a5cc0d48b8d7b22a481c88e06dd59db48b11abc6` |
| `phase12/12b-save-load-lifecycle` | `2119a7f0c278c3ed40ede44986d6859ab4e8e860` |

## Divergent histories and decisions

All seven histories are preserved in `retired-branches.bundle`, including
every unique commit and its objects. Preservation is not gameplay acceptance
and does not make obsolete contracts current. The bundle also preserves PR
#74's head, whose branch was already absent from the remote branch list.

| Branch / PR | Head | Disposition |
|---|---|---|
| `fix/data-driven-map-presentation` | `3a4ac0117d32c2a7d4dc6ced3af3b5ee4a048dd4` | Runtime map catalog/build selection is present in master. Client build, presentation module, and MAP2 environment blobs match exactly; remaining modules reflect later canonical map-owner/content work. Preserve the full patch history instead of overwriting those later changes. |
| `phase12/mmorpg-continuity-scope-audit` / #115 | `dce0d577f419edfa8d0d2c12b01f5cbc4ba60603` | Archive the unique scope document as `PHASE_12_MMO_SCOPE_AUDIT.md` here. Its old file-WAL/drop-recovery and proposed 12D–12F plan do not override the revised PostgreSQL contract or current roadmap. Current continuity exit concerns remain explicitly open. |
| `phase12/reconcile-postgres-continuity` | `45596ce0ecde61d1b2ce1ac4729458367a9a6908` | Reconciliation is on master as `d191f7a`, with later storage, lifecycle, HP, and ground-reset decisions. Do not restore stale review status or pre-implementation prose. |
| `phase12/12a-durable-domain` / #116 | `ddbf9a9bb98c0adb17552839f576eb9603d9470f` | Intentionally retire the unmerged file-backed writer. ADR-0069 and later PostgreSQL decisions supersede it. Full source/tests/history remain recoverable from the bundle; do not merge it into live persistence. |
| `phase12/12c-gameplay-durable` | `417dac268ff30e6c0e1bb7f46dc4a2dd73b80817` | Its accepted 12C base is merged, but `clouds extra` was not. Restore 19 art/tileset files to master. Preserve its Map 3 authoring and runtime snapshots here and in the bundle; activation is deferred until canonical Map Lab projection and runtime verification. Keep the newer database-env example on master. |
| `pre12/quest-domain-readiness` | `fe637dd34fd34c8713a606e99447159debb2a896` | Research is present as `1e4b044`; PostgreSQL reconciliation updates it. Quest gameplay remains a proposal. Older ordering and file-recovery conflict language is superseded, not a new feature to merge. |
| `tools/asset-slicer` | `111c97777dbc646ab7f6296dabd3c1c2fe107a8c` | All three slicer-tool blobs match master exactly; Hub Content/Dashboard/launcher entry points remain present after later tool additions. |
| PR #74, former `acceptance/ui-budget-overflow-55` | `4477576e7072a739f4adf4aebc28ab58365000e2` | Salvaged into master as `783e0a6`; requested/accepted counts, non-spamming overflow reporting, caps, and overflow tests are present. Current renderer also handles later textured quads. Preserve the old PR history, not a stale renderer replacement. |

## Restored and deferred map work

Restored files are exact blobs from `417dac2`:

- `Graphic/assets/rocks/1.png` through `8.png` (8).
- `Graphic/assets/skys/cloud_near/3.png` through `9.png` (7).
- `Graphic/assets/trees/1.png` through `3.png`, plus `Trees.tsx` (4).

`map.map3.gameplay.json` here contains **11 foothold paths** and the default
spawn `[19.673315, 1.0]`. `map.map3.runtime.json` is its historical shared-map
snapshot. The original runtime snapshot has no platform projection despite
those authored paths. Copying only that snapshot into live content is not a
verified map compilation. Preserve both intact; use the current canonical
Map Lab save/compile path and verify the intended map before activating them.
They do not currently overwrite `content/authoring` or `content/shared`.

The scope-audit snapshot is historical evidence. Read current `docs/ROADMAP.md`,
`docs/PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md`, and
`docs/PERSISTENCE_AND_AUTHORING_CONTRACT.md` for active policy.

## Recover exact histories

Bundle SHA-256:
`b8e9d76615974782ba7a75a375724b37247875a840933eb52219999a0fde6278`.
Size: 432,012 bytes. It is an **incremental** bundle, not a complete repository
or a PostgreSQL backup. It requires ancestor history already reachable from
audited master. A shallow clone must fetch that history before restoration.

Run in a full clone of this repository:

```bash
git bundle verify docs/archive/branch-cleanup-2026-10-03/retired-branches.bundle
git bundle list-heads docs/archive/branch-cleanup-2026-10-03/retired-branches.bundle
git fetch docs/archive/branch-cleanup-2026-10-03/retired-branches.bundle 'refs/remotes/origin/*:refs/heads/archive-20261003/*' 'refs/archive/pr-74:refs/heads/archive-20261003/pr-74'
```

Those commands restore local historical branches. They do not merge obsolete
code, write to the database, or push anything. After inspection, delete local
archive branches as needed; the committed bundle remains available. Bundle
verification and a fresh-repository restore were performed before publication.

## Cleanup boundary

These 20 branch refs may be removed after this preservation/consolidation
commit reaches master and a final head recheck. Close #115 and #116 as
archived/superseded and #74 as already salvaged; do not merge stale pull
requests simply to clear the list. Any new branch or advanced SHA needs its
own audit. A cleanup result must separately report what was actually deleted.

This work does not mark Item Lab or Phase 12 fully accepted. See
[`../../ITEM_LAB_CLOSEOUT_2026-10-03.md`](../../ITEM_LAB_CLOSEOUT_2026-10-03.md).
