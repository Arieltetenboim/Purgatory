# Monster drops

Schema version stays 4. `drops` may be omitted; that means an empty list and no hidden reward. Each entry is:

```json
{ "item": 30015, "chance_bps": 10000, "quantity_min": 2, "quantity_max": 2 }
```

`chance_bps` is an integer from 0 to 10000. 10000 is 100%. The Mob Lab field is percent, and `percent * 100` is the stored basis-point value. Each entry is rolled once per eligible monster death. Rows are independent, so several may succeed and the chances need not sum to 100. 0 never succeeds. 10000 always succeeds. A success chooses an integer uniformly from `quantity_min` through `quantity_max`. Minimum is at least 1, maximum is at least the minimum, and maximum cannot exceed the item stack limit. A non-stackable item accepts quantity 1 only. The same item cannot appear twice in one list. A missing item or an invalid quantity rejects the monster.

Only a real lethal hit from a player creates a plan. The killer is that damage source, resolved to `CharacterId` while the binding still exists, and stored on the plan. `npc.target` is not the killer. A death with no player killer creates no loot. Surviving hits, player death, logout, admin despawn, server stop, and deleting a live entity do not. One plan is emitted for one monster life. A later respawn is a new life.

Manifestation uses the existing reserved-id pool and `manifest_monster_loot`. The tick does not call the database. An empty pool leaves the unmanifested rows queued and does not mint a fallback id. A later refill manifests those same rows. It does not reroll and it does not create a second copy of a row already manifested. The queue holds at most 32 plans; a new plan past that cap is abandoned.

If the map address is closed, or the channel stops, while rows are still queued, the unmanifested remainder is abandoned. Items already on the ground keep the existing 40-second exclusive window and 200-second lifetime. They are not refunded. Ordinary ground is not restored after a process restart.

A failed chance roll enqueues nothing and is distinct from an allocation failure.
