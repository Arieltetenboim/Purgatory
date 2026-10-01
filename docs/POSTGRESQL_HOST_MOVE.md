# Moving PURGATORY's database off the development PC

The location of PostgreSQL is an operational choice. The game server uses
`PURGATORY_DATABASE_URL` (runtime role), optional
`PURGATORY_DATABASE_MIGRATION_URL` (migration role),
`PURGATORY_DATABASE_SCHEMA`, and `PURGATORY_DEPLOYMENT_ID`. A local database, a dedicated database host,
and a managed service all use this same persistence-worker boundary. There
is currently **one logical game world and one game database**; channels share
it. Adding a host or replica does not create another world or another writer.

This is a **planned move procedure**, not a claim that production failover,
backup recovery, or Internet exposure has been tested. The first local
connection and the chosen **fresh development reset** are described in
[`POSTGRESQL_LOCAL_RUN.md`](POSTGRESQL_LOCAL_RUN.md). The continuity rules are
in [`PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md`](PHASE_12_POSTGRESQL_CONTINUITY_CONTRACT.md).
This initial reset applies only to the first development cutover. **A later
move from local PostgreSQL to another database host preserves all committed
characters and items; it does not start a new world or reset player data.**

## Before selecting the destination

- Choose whether PostgreSQL runs on the same machine as the game server, on
  a dedicated private host, or as a managed database. Use stable hostnames,
  storage with reliable sync, monitored disk space, restricted network
  access, and a tested restore procedure. A public database port is not
  required for clients: only the game server and an authorized administrator
  connect to PostgreSQL.
- Keep the runtime and migration roles distinct, neither a superuser. The
  migration role owns the application schema and applies the migrations;
  the runtime role gets only the privileges the server grants it. Deliver
  both connection strings to the server through controlled secrets, outside
  Git, build artifacts, and logs. Rotate them using a rehearsed procedure.
- For a remote connection, use a Domain Name System (DNS) name matching the
  server certificate and `sslmode=verify-full`; the certificate authority
  must be trusted by the game-server machine. The current Rust client
  constructs a native Transport Layer Security (TLS) connector from the
  operating-system trust store. Test that the connection
  refuses a wrong hostname or an untrusted certificate. Restrict the
  database listener, firewall, and PostgreSQL host authentication to the
  game-server/admin network. Never reuse the local example's
  `sslmode=disable` remotely.
- Confirm `fsync`, `synchronous_commit`, and `full_page_writes` are on.
  Measure database connection count, worker queue pressure, commit latency,
  migration duration, restart cleanup, and restore time under a representative
  workload. A successful local test does not establish international-player
  capacity or power-loss durability.
- Decide the acceptable recovery point and recovery time before production.
  A backup or asynchronous replica behind an acknowledged item transaction
  cannot provide zero loss of acknowledged commits. Rehearse a restore and
  prove which committed transaction it reaches. PostgreSQL's base backup,
  write-ahead log archiving, and replication need explicit operational
  configuration; this repository does not configure or monitor them for you.

## Planned local-to-remote cutover

1. Rehearse on disposable data first. Prepare the destination PostgreSQL
   server, roles, schema privileges, trusted certificate, firewall rules,
   backups, and monitoring. Match the source server's supported PostgreSQL
   version and installed migration history. Do not point the ignored
   integration-test suite at the development or production database.
2. Announce a maintenance window and stop new player admission. Stop the
   **game server**, wait for the persistence worker's shutdown result, and
   record whether it drained, timed out, or closed. Resolve unknown durable
   command outcomes from their command keys before claiming a clean handoff.
   Fence the old game process and the old database writer; there must not be
   two independently writable copies of the game data.
3. Take a consistent source backup and restore it to the destination with an
   appropriate PostgreSQL procedure. The exact physical/logical backup
   commands depend on the chosen host and recovery design; do not copy raw
   database files from a running service. Keep the source protected and
   read-only during validation. Do not perform another import from old
   `identity.json` / `char_*.json` files: the game data now lives in
   PostgreSQL.
4. Before allowing traffic, compare the source and destination schema
   migration history, `deployment_id` and `cutover` in `durable_meta`, character
   and item counts, representative character identities, item owners and
   retired item identifiers (IDs), durable command keys, and channel/character
   lease rows. Verify that the destination contains the last acknowledged
   economic transactions.
   A count alone cannot prove ownership or the recovery point. Rehearse
   restoration of a selected character and a retry of a stored command key.
5. Configure the **same game server** with new runtime and migration URLs,
   the matching schema name, and the same `PURGATORY_DEPLOYMENT_ID` stored in
   `durable_meta`. Do not copy `durable_writer.json` or development characters
   into a newly initialized database. A missing or different
   deployment id refuses to start and does not create an empty roster. Do not
   run `--bootstrap-postgresql` against the restored database. Start one
   server, verify the migration check, channel claim, ground retirement,
   normal-client login/reconnect, item ownership, and logs, and only then
   reopen admission.
6. Keep the previous database offline or fenced until rollback is decided.
   After new writes are accepted on the destination, switching back to a
   stale copy would lose acknowledged items. Rollback then requires a
   separately designed data reconciliation or restore to a known recovery
   point, not just changing the URL back.

The live ground-drop timers are runtime state: ordinary unclaimed ground is
retired at startup rather than restored, while character-owned items and
their instance IDs come from the committed database. A server restart loads
ordinary monster populations from map content, not from a death counter.

## Operational ownership and failure behavior

| Failure or change | Required behavior |
|---|---|
| Database unreachable, wrong schema, or wrong deployment id at startup | Do not admit gameplay. The server does not open the file writer and does not create an empty roster. Investigate the connection, role, certificate, migration, and `PURGATORY_DEPLOYMENT_ID`. |
| Database reply lost after an economic commit | Retry the **same** durable command key; an unknown result is not evidence that the transaction rolled back. |
| Runtime connection loses lease/authority | Stop affected gameplay. Another process can claim only after release or expiry according to Architecture Decision Record (ADR) 0071 in [`DECISIONS.md`](DECISIONS.md); do not bypass fencing. |
| Database host replaced | Keep one durable writer and validate the exact recovery point before admission. DNS or URL changes alone do not move committed rows. |
| Scheduled event later requires persistence | Give it explicit per-channel state in a future domain migration. Do not make a database per channel. |

Production admission, authentication, backup restore point, replica promotion,
failover, and normal-client recovery still require their own evidence. This
document records the move boundary without treating an installed PostgreSQL
server or a successful connection as that evidence.

Official PostgreSQL references: [connection strings](https://www.postgresql.org/docs/18/libpq-connect.html),
[TLS and hostname verification](https://www.postgresql.org/docs/18/libpq-ssl.html),
[write-ahead log settings](https://www.postgresql.org/docs/18/runtime-config-wal.html),
and [backup and restore](https://www.postgresql.org/docs/18/backup.html).
