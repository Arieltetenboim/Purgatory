# Item Lab V1 verification

Base: `origin/master` `5397a71`. Branch: `forge/item-lab-v1`. The local checkout at `Purgatory/1` was not modified.

## Automated

- Monster death with two attackers keeps the lethal character, manifests the reserved id and quantity, and pickup keeps that id.
- Disconnect after the killing hit and before manifestation still attributes the drop to the stored `CharacterId`.
- A partial id pool manifests the rows that fit and leaves the remainder. The next refill does not reroll or duplicate.
- Closing the world address abandons only the unmanifested remainder.
- An empty drop list and a non-death despawn create nothing.

The Windows graphical client session is **NOT RUN**. That does not replace the automated chain above.

## Still manual

Open the rebuilt Hub, launch Item Lab, save an item, edit a Mob Lab drop graph, rebuild or restart, and kill the fixture monster in a dev session.
