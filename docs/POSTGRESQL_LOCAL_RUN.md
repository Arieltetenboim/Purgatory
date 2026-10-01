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

**Why does this guide mention a directory?** The current server still has a
legacy file-to-PostgreSQL import path. It scans `PURGATORY_DATA_DIR` on first
open and writes `durable_writer.json` there to fence its old file writer.
Characters, items, commands, and leases are stored in PostgreSQL after the
cutover. The directory is a compatibility dependency of this build, not a
second game database or part of the desired fresh-start design. A database-only
bootstrap that removes this dependency requires a separate code change.

## 1. Start a new development roster, without importing old characters

The chosen transition to PostgreSQL **resets development game data**. Stop
every running game server. **For the current implementation**, use both an
empty directory to avoid the legacy import and a new empty application
schema. The existing `Purgatory_dev` database can stay if it contains no
active PURGATORY schema at the chosen name. If
`purgatory_game` already contains game tables, choose a different new,
lowercase schema name and substitute it in the connection settings and
verification queries below. Never point a new empty directory at an
already-cut-over schema: the missing `durable_writer.json` marker is correctly
rejected. Do not run two servers against old and new schemas at once.

Set `PURGATORY_DATA_DIR` explicitly for both direct and Developer Hub starts
while the legacy bootstrap is still present. For example, choose
`%LOCALAPPDATA%\Purgatory-postgres-fresh` **only if it does not exist or is
empty**. A direct file-mode server normally uses
`%LOCALAPPDATA%\Purgatory`, while Developer Hub normally uses
`logs/dev-tools/hub_server_persist`; neither old directory should be reused
for this reset. The server sees no `identity.json` or `char_*.json` in the new
directory and records a fresh, empty roster. There is no import or merge.
After a confirmed fresh start, create new characters through the game.

Preserve the old development data and any previous trial schema until the
empty start and new-character flow have been verified. Then archive and
remove the old development data as a **separate, deliberate cleanup**. No
files, rows, or schemas are deleted by this guide. This reset is permitted
because these are development records, not production player accounts.
Once the new schema contains real game progress, do not repeat this reset
or switch to another empty schema to work around a connection error.

On first open the server writes `durable_writer.json` into the **new** data
directory before its database transaction commits. A marker alone does not
prove that initialization succeeded. Later starts require the matching
marker and committed cutover row. Do not delete or invent the marker. The
historical import behavior, if ever needed for a different environment, is
documented in [`PHASE_12A_POSTGRESQL.md`](PHASE_12A_POSTGRESQL.md#cutover).

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
$env:PURGATORY_DATABASE_URL = "postgresql://purgatory_dev:$runtimePassword@127.0.0.1:5432/Purgatory_dev?sslmode=disable"
$env:PURGATORY_DATABASE_MIGRATION_URL = "postgresql://purgatory_migrator:$migratorPassword@127.0.0.1:5432/Purgatory_dev?sslmode=disable"
$env:PURGATORY_DATABASE_SCHEMA = 'purgatory_game'
$env:PURGATORY_DATA_DIR = Join-Path $env:LOCALAPPDATA 'Purgatory-postgres-fresh'
Remove-Variable runtimePassword, migratorPassword, secure -ErrorAction SilentlyContinue
```

Before starting, confirm the path in `PURGATORY_DATA_DIR` does not exist or
is empty. If the schema name `purgatory_game` was already initialized, replace
it consistently here and in the pgAdmin queries below with the new name.
`sslmode=disable` is only for this loopback development connection. Never
reuse these local connection URLs (Uniform Resource Locators) for a remote
host. On later launches, keep the same database, schema, data directory, and
role identities; supply passwords again through
your chosen local secret mechanism. Setting only the migration URL without
`PURGATORY_DATABASE_URL` does not enable PostgreSQL mode.

From this PowerShell window, choose one launch method:

```powershell
cargo run -p purgatory-server
```

or launch `./DEV_HUB.BAT` and start the server from Developer Hub. The Hub
and its server inherit the environment **when the Hub starts**. If the Hub
was already running when you set these variables, close that Hub/server and
start a new Hub from this window. Run the client normally afterward; it
does not need database credentials.

On the first open, the migration login applies versioned migrations
`0001`–`0003` to `purgatory_game`, grants the runtime role its row privileges,
and the worker commits a fresh empty roster, because the new directory has
no identity files. On subsequent opens, it verifies migration history and
uses the existing rows. A failed connection or initialization is an error,
not permission to silently return to the
file writer. The server claims its channel generation before admitting
gameplay. Visible collectible developer spawns in 12C require this PostgreSQL
allocator for item identifiers (IDs); they do not appear in file mode.

## 4. Confirm the server is using the database

In pgAdmin, refresh **Databases → Purgatory_dev → Schemas → purgatory_game →
Tables**. Open **Tools → Query Tool** on `Purgatory_dev` and run:

```sql
SELECT version FROM purgatory_game.schema_migrations ORDER BY version;
SELECT value FROM purgatory_game.durable_meta WHERE key = 'cutover';
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
The `cutover` value should say `fresh`. Check the game-server log for a
persistence-open or channel-claim error before interpreting tables as a
successful gameplay start. `pgAdmin` may show rows only after refreshing.
It does not itself make the server use the database.

## When something fails

| Symptom | Check |
|---|---|
| Password authentication failed | Exact role spelling, the password of **that role**, database name/case, and the pgAdmin server host/port. Do not change `pg_hba.conf` to `trust`. |
| Permission denied for schema/table | The two URLs must name different roles. Check database `CONNECT`, migrator `CREATE`, schema ownership, and whether the server applied the runtime grants. Do not make the runtime role a superuser. |
| No `purgatory_game` schema | The Hub may have started before the environment was set, the server may still be in file mode, or migration may have failed. Check its log and the environment in the launching PowerShell; do not paste URLs into logs. |
| Missing `durable_writer.json` / cutover mismatch | The current build requires the same directory used at first PostgreSQL start, even for a fresh roster. Do not copy a marker from an unrelated database or delete the database's cutover row. |
| Another channel generation still active | Stop the other server cleanly; after a crash, wait for the lease expiry as described in [`PHASE_12B_LIFECYCLE.md`](PHASE_12B_LIFECYCLE.md). Do not force a second active process. |
| Range empty; collectible spawn absent | Verify PostgreSQL mode, live channel claim, and persistence-worker health. The simulation tick does not allocate IDs or fall back to epoch IDs. |

This guide does not certify a live client recovery session or production
backup/restore. Those are separate Phase 12 exit checks. For moving the
database to another host, see [`POSTGRESQL_HOST_MOVE.md`](POSTGRESQL_HOST_MOVE.md).
