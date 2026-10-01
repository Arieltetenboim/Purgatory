# Run PURGATORY with local PostgreSQL (Windows)

This is the development runbook for the **one** game database. PostgreSQL 18
currently runs on the development PC; `pgAdmin` is a graphical administration
client, not the database server. Channels share this database. The game server
connects through the persistence worker; the native client connects to the
game server, never to PostgreSQL. These instructions describe the current
`phase12/12c-gameplay-durable` branch, while Phase 12C is in review. A checkout
that does not contain migration `0003_item_id_reservations.sql` will not show
version 3 or this branch's collectible-ID behavior.

The database in the examples is the existing `Purgatory_dev` on `127.0.0.1:5432`.
Use the actual role and database names shown in pgAdmin: PostgreSQL preserves
the case of a name created with quotes. **Do not** run the ignored integration
tests against `Purgatory_dev`; they require a disposable test database.

Game data is stored only in PostgreSQL. Server startup does not create the
database, apply migrations, or read `identity.json`, `char_*.json`, or
`durable_writer.json`. Those file instructions are superseded (ADR-0073).
Importing an old development save is not a supported setup step.

## 1. Create the local database from Developer Hub

The pinned target is the physical database `Purgatory_dev` on `127.0.0.1`.
Hub Create, when that database is absent, creates it, applies the versioned
migrations, grants the runtime role, and inserts the reserved readiness login
`dev.probe` plus the first development username you type. That user starts
with zero characters. Create does not overwrite a database that is already
initialized; add another username with Add Development User instead.

In pgAdmin the tables are under `Purgatory_dev` → Schemas → `purgatory_game`
→ Tables. Expect `schema_migrations`, `durable_meta`, `dev_users`,
`characters`, `item_instances`, `character_leases`, `channel_generations`,
`item_id_reservations`, and the narrative tables from the migrations.

Reset asks you to type `Purgatory_dev`. It stops the Hub-managed server,
waits for a drained persistence shutdown, drops only that database, and
creates a new empty world. Users, characters, items, leases, command results,
and reservations are deleted. If recreation fails after the drop, Hub reports
the database as absent and does not start the server.

The historical `import_legacy_postgresql` path and
`--bootstrap-postgresql` are superseded. Do not use them as setup steps.

Bootstrap does not read the old `%LOCALAPPDATA%\Purgatory` directory or the
Developer Hub directory `logs/dev-tools/hub_server_persist`. Old
`identity.json` and `char_*.json` records are not imported. After a confirmed
fresh start, create new characters through the game.

Preserve the old development files and any previous trial schema until the
empty start and new-character flow have been verified. Then archive and
remove the old development data as a **separate, deliberate cleanup**. No
files, rows, or schemas are deleted by this guide. This reset is permitted
because these are development records, not production player accounts.
Once the new schema contains real game progress, do not run bootstrap again.
A second bootstrap fails and does not erase characters or items. Do not switch
to another empty schema to work around a connection error.

Choose one deployment id, for example `purgatory-dev`, and keep it. It is 1
to 64 characters: ASCII letters, digits, `.`, `_`, and `-`. Bootstrap stores
it. A later start with a different id, URL, or schema fails closed. The
historical import, if ever needed for a different environment, is
`import_legacy_postgresql` and is not this startup path. Its older steps are
in [`PHASE_12A_POSTGRESQL.md`](PHASE_12A_POSTGRESQL.md#cutover).

## 2. Check the local service and roles in pgAdmin

1. In Windows PowerShell, run `Get-Service postgresql-x64-18`. Its status
   should be `Running`. If this installation has a different service name,
   find it with `Get-Service 'postgresql*'`.
2. Open pgAdmin. Expand **Servers → PostgreSQL 18 → Databases** and confirm
   `Purgatory_dev` exists. If the server registration has a different label,
   select the registration that connects to `127.0.0.1:5432`.
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
5. Select **Databases → Purgatory_dev**, then **Tools → Query Tool**. While
   connected as an administrator, run the following grants. They allow the
   migrator to create the dedicated `purgatory_game` schema; the server's
   migration code grants the runtime role access to its tables afterward.
   In pgAdmin, expand **Purgatory_dev → Schemas** and confirm the chosen
   schema name is not already an initialized game schema. If it is, select
   another fresh name before proceeding; do not clear existing rows to make
   this example work.

   ```sql
   GRANT CONNECT ON DATABASE "Purgatory_dev" TO purgatory_dev, purgatory_migrator;
   GRANT CREATE ON DATABASE "Purgatory_dev" TO purgatory_migrator;
   ```

   Check the database owner under **Purgatory_dev → Properties → General**.
   If it is `purgatory_dev`, first transfer ownership to an administrator
   rather than assuming a `REVOKE` removes an owner's implicit privileges.
   In that **General** tab, select the administrator (normally `postgres`)
   in **Owner** and click **Save**, then reopen the properties to verify it.
   The runtime role must not own the database or `purgatory_game` schema.
   Do not grant it `CREATE` on the database or schema. If the roles or grants
   already exist, inspect them; do not recreate the database or reset data.

## 3. Give the server its connection settings

Open a **new PowerShell window in the repository root**. The following
example prompts for passwords without echoing them or putting literals in
PowerShell history. The connection URLs remain plaintext environment
variables in this process and its children, so this is a development setup,
not a production secret-distribution system. `EscapeDataString` encodes URL
characters such as `@` and `:` in the passwords.

```powershell
function Read-EncodedPassword([string]$Prompt) {
    $secure = Read-Host -Prompt $Prompt -AsSecureString
    $ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {
        [uri]::EscapeDataString([Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr))
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr)
    }
}

$runtimePassword = Read-EncodedPassword 'purgatory_dev password'
$migratorPassword = Read-EncodedPassword 'purgatory_migrator password'
$adminPassword = Read-EncodedPassword 'postgres administrator password'
$env:PURGATORY_DATABASE_URL = "postgresql://purgatory_dev:$runtimePassword@127.0.0.1:5432/Purgatory_dev?sslmode=disable"
$env:PURGATORY_DATABASE_MIGRATION_URL = "postgresql://purgatory_migrator:$migratorPassword@127.0.0.1:5432/Purgatory_dev?sslmode=disable"
$env:PURGATORY_DATABASE_ADMIN_URL = "postgresql://postgres:$adminPassword@127.0.0.1:5432/postgres?sslmode=disable"
$env:PURGATORY_DATABASE_SCHEMA = 'purgatory_game'
$env:PURGATORY_DEPLOYMENT_ID = 'purgatory-dev'
Remove-Variable runtimePassword, migratorPassword, adminPassword, secure -ErrorAction SilentlyContinue
```

If the schema name `purgatory_game` was already initialized, replace it
consistently here and in the pgAdmin queries below with the new name.
`sslmode=disable` is only for this loopback development connection. Never
reuse these local connection URLs (Uniform Resource Locators) for a remote
host. On later launches, keep the same database, schema, deployment id, and
role identities; supply passwords again through
your chosen local secret mechanism. A missing
`PURGATORY_DATABASE_URL` or `PURGATORY_DEPLOYMENT_ID` stops startup. Setting
only the migration URL does not open the file writer.

Create the database from Developer Hub, not from ordinary server startup.
Launch `./DEV_HUB.BAT` from this window so it inherits the three URLs. In the
database section, type the first development username and press Create.
Success leaves the database Ready. Start Server, then open the normal client
and enter that same username. The roster is empty until you create a
character in the client. An unregistered username is rejected on the login
screen.

The same operations exist headless, and they still refuse every database
except `Purgatory_dev`:

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

`--bootstrap-postgresql` is superseded by `--database-create`.

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

Expect migration versions `1`, `2`, and `3` on this branch. Compare character
count immediately after initialization: it should be **zero** before a new
character is created, and then increase only as new characters are created.
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

This guide does not certify a live client recovery session or production
backup/restore. Those are separate Phase 12 exit checks. For moving the
database to another host, see [`POSTGRESQL_HOST_MOVE.md`](POSTGRESQL_HOST_MOVE.md).
