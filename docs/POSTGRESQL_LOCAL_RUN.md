# Run PURGATORY with local PostgreSQL (Windows)

This is the development runbook for the **one** game database. PostgreSQL 18
currently runs on the development PC; `pgAdmin` is a graphical administration
client, not the database server. Channels share this database. The game server
connects through the persistence worker; the native client connects to the
game server, never to PostgreSQL. These instructions describe current
`master`: Phase 12A, 12B, and 12C are accepted, while the separate Phase 12
exit remains open. A checkout that does not contain migration
`0004_current_health.sql` will not show version 4 or stored current HP.

The database in the examples is the existing `Purgatory_dev` on `127.0.0.1:5432`.
Use the actual role and database names shown in pgAdmin: PostgreSQL preserves
the case of a name created with quotes. **Do not** run the ignored integration
tests against `Purgatory_dev`; they require a disposable test database.

Game data is stored only in PostgreSQL. Server startup does not create the
database, apply migrations, or read old development saves. Historical file
import is superseded (ADR-0073) and is not a setup step.

## 1. Create the database, add a user, start the server, open the client

Do these steps in order. The only database this guide creates or deletes is
the pinned local database `Purgatory_dev` on `127.0.0.1`.

1. Create the roles in section 2 if they do not exist, then create the local
   file in section 3 once. Launch `./DEV_HUB.BAT` by double-click or from any
   shell. Developer Hub reads `config/local/database.env` itself. A PowerShell
   window is not required, and closing a shell does not drop the settings.
2. In the database section, type the first development username and press
   **Create**. When `Purgatory_dev` is absent, Create creates that physical
   database, applies the versioned migrations, grants the runtime role, and
   inserts only the username you typed. That user starts with zero characters.
   `dev.probe` is the readiness probe. It is not a row in `dev_users` and it
   cannot create a character. Create on a database that is already initialized
   applies only a pending migration tail. It does not drop users, characters,
   or items, and it does not insert another user. A database that is already
   at the current migration stays as it is. Reset is the operation that
   deletes the database.
3. Press **Start Server**. Startup opens the existing database. It does not
   create, reset, or migrate it.
4. Open the normal client and enter that same username. The roster is empty
   until you create a character in the client. An unregistered username is
   rejected on the login screen. Typing `dev.probe` is rejected the same way.

Add another development user later with **Add Development User**. That does
not reset the database.

In pgAdmin the tables are under `Purgatory_dev` → Schemas → `purgatory_game`
→ Tables. After Create, expect `schema_migrations`, `durable_meta`,
`dev_users`, `characters`, `item_instances`, `character_leases`,
`channel_generations`, `item_id_reservations`, and the narrative tables.
`dev_users` contains only usernames you provisioned. `characters` stays empty
until the client creates one.

**Reset** asks you to type `Purgatory_dev`. It stops the Hub-managed server
and continues only after that process reports a drained shutdown with zero
save failures. It then deletes that database and creates a new empty one.
Users, characters, items, leases, command results, and reservations are gone.
An old "shutdown drained" line in `server.log` does not authorize the delete.
If recreation fails after the drop, Hub reports the database as absent and
does not start the server.

The deployment id in the example is `purgatory-dev`. Keep the same id, URL,
and schema on later starts. A different id, URL, or schema fails closed.

## 2. Check the local service and roles in pgAdmin

1. In Windows PowerShell, run `Get-Service postgresql-x64-18`. Its status
   should be `Running`. If this installation has a different service name,
   find it with `Get-Service 'postgresql*'`.
2. Open pgAdmin and select the server registration that connects to
   `127.0.0.1:5432`. `Purgatory_dev` appears under **Databases** after Create
   in section 1. It is not required to exist before that.
3. Expand **Login/Group Roles**. The existing `purgatory_dev` role is the
   suggested runtime login. It must have **Can login = Yes** and
   **Superuser / Create databases / Create roles = No**. Its password is the
   one you verified by logging in as that role; pgAdmin never displays it
   afterward. A role that owns the game database or schema also has implicit
   create powers and is unsuitable as the restricted runtime role.
4. Create a *different* login for migrations if it does not exist: right-click
   **Login/Group Roles → Create → Login/Group Role**. On **General**, name it
   `purgatory_migrator`; on **Definition**, set a new password; on
   **Privileges**, enable **Can login**, leave **Superuser**, **Create roles**,
   and **Create databases** off; click **Save**. Keep both passwords out of
   the repository, scripts, and screenshots.
5. Hub Create applies `CONNECT` for both roles and `CREATE` for the migrator.
   After Create, you can confirm that in pgAdmin's Query Tool on
   `Purgatory_dev`, connected as an administrator:

   ```sql
   GRANT CONNECT ON DATABASE "Purgatory_dev" TO purgatory_dev, purgatory_migrator;
   GRANT CREATE ON DATABASE "Purgatory_dev" TO purgatory_migrator;
   ```

   The runtime role must not own the database or `purgatory_game` schema, and
   it must not have `CREATE` on either. If **Properties → General → Owner** is
   `purgatory_dev`, transfer ownership to the administrator (normally
   `postgres`) and save. Do not use Reset to repair a grant. Reset deletes the
   database.

## 3. Create the local configuration file once

The file is `config/local/database.env` in the repository root. It is listed
in `.gitignore`. Do not commit it, paste it into chat, or put it under
`config/local/database.env.example`.

1. Copy `config/local/database.env.example` to `config/local/database.env`.
2. Replace `REPLACE_RUNTIME_PASSWORD` with the `purgatory_dev` password and
   `REPLACE_MIGRATION_PASSWORD` with the `purgatory_migrator` password. If a
   password contains `@`, `:`, `/`, or `%`, percent-encode that character in
   the URL (`@` is `%40`, `:` is `%3A`). Quote the whole value with `"` when
   it contains spaces. `#` inside a value is part of the value.
3. Leave the database name `Purgatory_dev`, the schema `purgatory_game`, the
   deployment id `purgatory-dev`, the host `127.0.0.1`, and `sslmode=disable`
   unless this machine was initialized with a different schema name. The file
   accepts only a loopback host. Do not add `PURGATORY_DATABASE_ADMIN_URL`.

`DEV_HUB.BAT` and Developer Hub load that file on every launch, including a
double-click. The game server and Check receive the runtime URL, schema, and
deployment id. Add User also receives the migration URL. Create and Reset
receive those settings plus the administrator password typed into the Hub
database panel. That password is kept in memory for the Hub session and is
not written to the file.

To change the settings, edit `config/local/database.env` and start Developer
Hub again. Restart the game server after a change; a server that is already
running keeps the environment it was given at launch.

Check and Start Server do not need the `postgres` administrator password.
Create and Reset do. If the password field is empty, Hub does not start those
operations. Headless Create and Reset still read
`PURGATORY_DATABASE_ADMIN_URL` from the environment of that command. The local
file does not supply it. When `PURGATORY_DATABASE_URL` is already set, the
process uses that environment and does not read the file.

A missing `PURGATORY_DATABASE_URL` or `PURGATORY_DEPLOYMENT_ID` stops startup.
Setting only the migration URL does not open a game database. `sslmode=disable`
is only for this loopback development connection. Never reuse these local
connection URLs (Uniform Resource Locators) for a remote host.

Section 1 is the Hub path. The same operations exist headless, and they still
refuse every database except `Purgatory_dev`:

```powershell
cargo run -p purgatory-server -- --database-create --user dev.player
cargo run -p purgatory-server -- --database-status
cargo run -p purgatory-server -- --database-add-user --user dev.friend
cargo run -p purgatory-server -- --database-reset --confirm Purgatory_dev
```

A later start checks the stored deployment id and the exact migration
history. It does not apply a newer migration and it does not create an empty
world. A failed connection, missing database, wrong schema or deployment id,
or unsupported migration history stops startup. The listening line is printed
only after the database opens. The runtime role cannot create or drop the
database and cannot insert into `dev_users`.

## 4. Confirm the server is using the database

In pgAdmin, refresh **Databases → Purgatory_dev → Schemas → purgatory_game →
Tables**. Open **Tools → Query Tool** on `Purgatory_dev` and run:

```sql
SELECT version FROM purgatory_game.schema_migrations ORDER BY version;
SELECT key, value FROM purgatory_game.durable_meta
WHERE key IN ('cutover', 'deployment_id', 'next_character_id', 'next_item_instance_id');
SELECT encode(character_id, 'hex') AS character_id, owner_login, display_name
FROM purgatory_game.characters ORDER BY owner_login, roster_position;
SELECT count(*) AS characters FROM purgatory_game.characters;
SELECT count(*) AS items FROM purgatory_game.item_instances;
SELECT name, setting FROM pg_settings
WHERE name IN ('fsync', 'synchronous_commit', 'full_page_writes');
```

Expect migration versions `1`, `2`, `3`, and `4` on this branch. Version 4
adds `characters.current_health` and `characters.health_revision`. Existing
rows keep `current_health` NULL until a changed HP value is saved. NULL loads
as that character's full maximum. Compare character count immediately after
initialization: it should be **zero** before a new character is created, and
then increase only as new characters are created. An upgrade must leave the
existing user, character, map, and item rows in place.
`cutover` should say `fresh` and `deployment_id` should say `purgatory-dev`,
or the id you configured. Check the game-server log for a
persistence-open or channel-claim error before interpreting tables as a
successful gameplay start. `pgAdmin` may show rows only after refreshing.
It does not itself make the server use the database.

## When something fails

| Symptom | Check |
|---|---|
| Password authentication failed | Exact role spelling, the password of **that role**, database name/case, and the pgAdmin server host/port. Do not change `pg_hba.conf` to `trust`. |
| Permission denied for schema/table | The two URLs must name different roles. Check database `CONNECT`, migrator `CREATE`, schema ownership, and whether the server applied the runtime grants. Do not make the runtime role a superuser. |
| No `purgatory_game` schema | Create has not been run, or it failed. Startup stops. It does not create the database and it does not write game files. Check the Hub database status and the launching environment; do not paste URLs into logs. |
| Database is missing, or deployment identity does not match | Run Create for `Purgatory_dev`, or the schema name differs, or `PURGATORY_DEPLOYMENT_ID` is not the stored id. Do not delete rows to force a new empty world. Reset is the deliberate wipe, and it requires the exact name `Purgatory_dev`. |
| Another channel generation still active | Stop the other server cleanly; after a crash, wait for the lease expiry as described in [`PHASE_12B_LIFECYCLE.md`](PHASE_12B_LIFECYCLE.md). Do not force a second active process. |
| Range empty; collectible spawn absent | Verify PostgreSQL mode, live channel claim, and persistence-worker health. The simulation tick does not allocate IDs or fall back to epoch IDs. |

## Windows client smoke with the existing `ariel` user

This checks the normal client against the already initialized `Purgatory_dev`.
Do not press **Create** or **Reset**, and do not run `--database-create` or
`--database-reset`. The `ariel` row is already in `dev_users`.

1. Create `config/local/database.env` as in section 3 if it is not already
   there. Launch `./DEV_HUB.BAT`. Do not set `PURGATORY_DATABASE_ADMIN_URL`
   for this smoke. Check and Start Server use the runtime role from the file.
2. Press **Check**. The status should be `ready`. The server state stays
   Stopped until you start it.
3. Press **Start Server**. The Hub may report Ready only after startup has
   opened PostgreSQL. `logs/dev-tools/server.log` contains
   `network listening on 127.0.0.1:5001` and
   `PURGATORY persist backend=postgresql`. A refused connection is
   `PURGATORY server error: persistence open: ...` and does not contain
   `Cannot start a runtime from within a runtime`.
4. After the server is Ready, press **+1 Client**, or run
   `cargo run -p purgatory-client` from that same environment. On the login
   screen enter `ariel`. The roster opens for that existing user. Do not
   type `dev.probe`.

This smoke does not by itself finish Phase 12. Client recovery and
backup/restore remain separate exit checks.

This guide does not certify a live client recovery session or production
backup/restore. Those are separate Phase 12 exit checks. For moving the
database to another host, see [`POSTGRESQL_HOST_MOVE.md`](POSTGRESQL_HOST_MOVE.md).
