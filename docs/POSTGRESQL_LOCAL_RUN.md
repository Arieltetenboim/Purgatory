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

## 1. Choose the source data before the first database start

Stop every running game server. Decide which existing development characters
to import. A direct server normally uses `%LOCALAPPDATA%\Purgatory`, while a
server launched by Developer Hub normally uses
`logs/dev-tools/hub_server_persist`. Inspect both locations before choosing.
From the repository root you can compare file names and dates without
printing character data:

```powershell
Get-ChildItem "$env:LOCALAPPDATA\Purgatory" -ErrorAction SilentlyContinue |
    Select-Object Name, Length, LastWriteTime
Get-ChildItem '.\logs\dev-tools\hub_server_persist' -ErrorAction SilentlyContinue |
    Select-Object Name, Length, LastWriteTime
```

Set `PURGATORY_DATA_DIR` to the chosen directory explicitly, and keep that
setting the same for both launch methods. Back up the chosen directory before
the first database start; do not edit it during import. An empty directory
starts an empty roster. There is no automatic merge of two different rosters.

The first PostgreSQL open writes `durable_writer.json` **before** its import
commits. A failed import may therefore leave this marker for a safe retry; it
does not by itself prove import success. After cutover, the character JSON
(JavaScript Object Notation) files are historical input, not a second writer.
Later starts against a committed database require its corresponding
marker; pointing the server at another, marker-free directory fails closed.
Do not delete or invent a marker to bypass this check. If the database was
already initialized, retain the same data directory and skip the one-time
import preparation. The exact supported input and failure rules are in
[`PHASE_12A_POSTGRESQL.md`](PHASE_12A_POSTGRESQL.md#cutover).

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
$env:PURGATORY_DATA_DIR = 'C:\path\to\the\chosen\Purgatory-data-directory'
Remove-Variable runtimePassword, migratorPassword, secure -ErrorAction SilentlyContinue
```

Replace the **data directory** placeholder with the path chosen in step 1.
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
and the worker imports the supported development roster once. On subsequent
opens, it verifies migration history and uses the existing rows. A failed
connection or import is an error, not permission to silently return to the
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
identities and counts with the **unchanged chosen source directory** after a
first import, as required by the cutover guide. An empty new roster can
legitimately report zero characters. Check the game-server log for a
persistence-open or channel-claim error before interpreting tables as a
successful gameplay start. `pgAdmin` may show rows only after refreshing.
It does not itself make the server use the database.

## When something fails

| Symptom | Check |
|---|---|
| Password authentication failed | Exact role spelling, the password of **that role**, database name/case, and the pgAdmin server host/port. Do not change `pg_hba.conf` to `trust`. |
| Permission denied for schema/table | The two URLs must name different roles. Check database `CONNECT`, migrator `CREATE`, schema ownership, and whether the server applied the runtime grants. Do not make the runtime role a superuser. |
| No `purgatory_game` schema | The Hub may have started before the environment was set, the server may still be in file mode, or migration may have failed. Check its log and the environment in the launching PowerShell; do not paste URLs into logs. |
| Missing `durable_writer.json` / cutover mismatch | Use the same data directory that performed the first import. Do not copy a marker from an unrelated database or delete the database's cutover row. |
| Another channel generation still active | Stop the other server cleanly; after a crash, wait for the lease expiry as described in [`PHASE_12B_LIFECYCLE.md`](PHASE_12B_LIFECYCLE.md). Do not force a second active process. |
| Range empty; collectible spawn absent | Verify PostgreSQL mode, live channel claim, and persistence-worker health. The simulation tick does not allocate IDs or fall back to epoch IDs. |

This guide does not certify a live client recovery session or production
backup/restore. Those are separate Phase 12 exit checks. For moving the
database to another host, see [`POSTGRESQL_HOST_MOVE.md`](POSTGRESQL_HOST_MOVE.md).
