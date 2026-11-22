# LILLY server deployment runbook

This runbook covers the production Compose stack in `deploy/`. The stack is deliberately isolated from the existing `maddrax-fanclub` and `nextcloud` Compose projects. Public HTTPS traffic is terminated by a shared Traefik reverse proxy that is operated outside this repository.

## Safety invariants

- Always pass `--project-name lilly` and the absolute LILLY Compose file.
- Never bind the LILLY stack itself to host ports 80 or 443.
- Never run a global Docker volume prune as part of deployment or backup cleanup.
- Never change the shared Traefik stack, its certificates or the labels of other Compose projects as part of a LILLY deployment.
- Never publish the LILLY host port on a public address; it is reserved for local health checks (`127.0.0.1:8091`).
- Keep `/opt/lilly/shared/.env.production` on the server only, with mode `0600`.
- Use `/opt/lilly/infrastructure/scripts/restore.sh` for recovery. Keep both release
  directories referenced by `current`, `infrastructure` and `shared/backup-tools` available.

## Supported host and first installation

The production path requires Linux with systemd, Docker Engine and Docker Compose
v2 (including `config --format json` and `pull --policy missing`), Python 3.12 or
later, Bash, `flock`, `gzip`, `tar`, `sha256sum`, `curl`, and `systemd-analyze`.
The automated container acceptance uses Debian 13; the operator's actual Linux
release and systemd version must be recorded in the host commissioning checklist.
The shared Traefik instance and DNS must meet the prerequisites below.

Start from a checkout of the intended full commit, outside `/opt/lilly`. Run
provisioning **before** creating any other file below `/opt/lilly`; it establishes
the live erasure ledger and refuses an unsafe empty replacement on existing hosts.
The following sequence makes release scripts available before configuration or
the first deployment needs them:

```bash
LILLY_RELEASE=$(git rev-parse HEAD)
sudo deploy/scripts/provision-server.sh /path/to/lilly-deploy.pub
sudo install -d -m 0750 -o lilly-deploy -g lilly-deploy "/opt/lilly/releases/$LILLY_RELEASE"
git archive "$LILLY_RELEASE" deploy |
  sudo -u lilly-deploy tar --extract --strip-components=1 --directory "/opt/lilly/releases/$LILLY_RELEASE"

sudo install -m 0600 -o lilly-deploy -g lilly-deploy \
  deploy/.env.smtp.example /opt/lilly/shared/.smtp-input
sudoedit /opt/lilly/shared/.smtp-input
sudo python3 "/opt/lilly/releases/$LILLY_RELEASE/scripts/configure-environment.py" \
  --smtp-config /opt/lilly/shared/.smtp-input \
  --output /opt/lilly/shared/.env.production \
  --app-base-url https://lilly.maddrax-fanclub.de
sudo rm /opt/lilly/shared/.smtp-input

sudo install -m 0600 -o lilly-deploy -g lilly-deploy \
  deploy/.env.operations.example /opt/lilly/shared/.env.operations
sudoedit /opt/lilly/shared/.env.operations
sudo python3 "/opt/lilly/releases/$LILLY_RELEASE/scripts/configure-operations.py" \
  --root /opt/lilly --apply

printf 'LILLY_IMAGE_TAG=%s\nLILLY_BIND_ADDRESS=127.0.0.1\nLILLY_HOST_PORT=8091\nLILLY_RESOURCE_PREFIX=lilly\n' "$LILLY_RELEASE" |
  sudo -u lilly-deploy sh -c 'umask 077; cat > /opt/lilly/shared/.deployment.env'
sudo -u lilly-deploy "/opt/lilly/releases/$LILLY_RELEASE/scripts/deploy.sh" "$LILLY_RELEASE"
```

Before the final command, the matching backend/frontend commit-tagged images
must have been published by GitHub Actions and be readable from the host. The
first workflow may stop at deployment until this bootstrap and the production
environment secrets are ready. Existing local immutable images are reused;
missing images are pulled. Do not substitute the mutable `main` tag.

Enter actual SMTP credentials and actual operations email recipients in the
private input files. The generator supports TLS on port 465 and STARTTLS on 587,
escapes literal dollar signs for Compose, and never overwrites an existing
production file, including concurrent setup attempts. `--mail-source` remains
available for the existing club installation as described below.

After HTTPS works, register and verify the operator account through the UI, then
promote that existing account without supplying its password:

```bash
docker compose --project-name lilly \
  --env-file /opt/lilly/shared/.env.production \
  --env-file /opt/lilly/shared/.deployment.env \
  --file /opt/lilly/infrastructure/docker-compose.production.yml \
  exec -T backend ./lilly-backend admin promote --email operator@example.org
```

Sign in again to receive the updated role. Verify an ordinary collection entry
and photo, a clean stop/start and a host reboot with the same data volumes.
Complete the backup/restore checks below before enabling the daily timer.

## Prerequisites: Traefik ingress

The LILLY `caddy` service joins the external Docker network `proxy` and announces itself to Traefik through container labels. Before the first deployment the host must provide:

- a running Traefik (v3) instance attached to the network `proxy` (`docker network create --subnet 172.30.0.0/24 proxy`),
- an entrypoint named `websecure` (TCP 443) and an ACME certificate resolver named `le`,
- a Docker provider limited to containers with `traefik.enable=true` (`exposedByDefault=false`),
- DNS `A` (and, if reachability was tested, `AAAA`) records for `lilly.maddrax-fanclub.de` pointing to the server.

`deploy.sh` fails early with a clear message when the network `proxy` does not exist. The public host name defaults to `lilly.maddrax-fanclub.de`; override it with `LILLY_PUBLIC_HOST` in `/opt/lilly/shared/.env.production` if required. Security headers (HSTS, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`) are set by a middleware defined in the container labels, so no shared Traefik file configuration is needed.

Traefik replaces `X-Forwarded-*` headers from untrusted clients by default. Do not configure `forwardedHeaders.trustedIPs` for the public entrypoint; the LILLY Caddy and backend only trust private ranges (`TRUSTED_PROXY_CIDRS=172.16.0.0/12`, which covers the `proxy` network).

## One-time GitHub configuration

Create a GitHub Environment named `production` without a required reviewer so pushes to `main` remain automatic. Add these Environment secrets:

| Secret                     | Value                                                      |
| -------------------------- | ---------------------------------------------------------- |
| `LILLY_SERVER_HOST`        | Server hostname or IP used for SSH                         |
| `LILLY_SERVER_USER`        | `lilly-deploy`                                             |
| `LILLY_SERVER_SSH_KEY`     | Dedicated Ed25519 private key                              |
| `LILLY_SERVER_KNOWN_HOSTS` | Trusted complete OpenSSH `known_hosts` line for the server |

The deploy job connects with `StrictHostKeyChecking=yes`. After reinstalling the server, either restore its previous SSH host keys or update `LILLY_SERVER_KNOWN_HOSTS` (for example from `ssh-keyscan -t ed25519 <host>`, verified out of band) before the next deployment.

After the first workflow image build, ensure both `lilly-backend` and `lilly-frontend` packages are linked to this repository and publicly readable in GHCR. The deploy job uses its ephemeral repository token for the first pull and logs out immediately afterward, so no long-lived package credential is stored on the server. Do not place application, MariaDB, JWT, SMTP, or OAuth secrets in GitHub.

## One-time server provisioning

Generate a dedicated Ed25519 key, transfer only its public part with the existing administrative account, and run:

```bash
sudo deploy/scripts/provision-server.sh /path/to/lilly-deploy.pub
```

This one-time provisioning creates the empty account-erasure ledger with owner/group `999:999`
and mode `0600`. Re-running it for an existing installation whose ledger is missing fails closed;
do not create an empty replacement, because it would lose the restore protection for completed
deletions. Recover the latest surviving live ledger instead.

Generate the private environment with the supplied helper. It creates unique URL-safe MariaDB passwords and a high-entropy JWT secret, writes the file atomically, and sets mode `0600` and ownership to `lilly-deploy`:

```bash
sudo /opt/lilly/releases/<RELEASE>/scripts/configure-environment.py \
  --mail-source /root/maddrax-fanclub/.env.production \
  --output /opt/lilly/shared/.env.production \
  --app-base-url https://lilly.maddrax-fanclub.de
```

Copy only the SMTP host, port, username, password and suitable sender from the club website's server-side environment into the LILLY environment. Map an existing implicit SSL/TLS setup on port 465 to `SMTP_TLS_MODE=tls`; use `SMTP_TLS_MODE=starttls` for a submission server on port 587. Do not source the other application's file at LILLY runtime and do not copy any unrelated value. Leave OAuth values and `ADMIN_EMAIL` empty until their intended values are available.

The generated environment also sets a one-hour password-reset lifetime, the documented rate-limit
defaults and `TRUSTED_PROXY_CIDRS=172.16.0.0/12` for the private Docker bridge. Do not add public
client networks to that trust list. Traefik replaces incoming `X-Forwarded-For` values from
untrusted clients; Caddy and the backend then parse the chain from right to left. If the Docker
network changes to a non-matching subnet, update the CIDR narrowly before enabling public traffic.

The helper sets `COOKIE_SECURE=true` for `https://` URLs. Keep both values in the production environment:

```dotenv
APP_BASE_URL=https://lilly.maddrax-fanclub.de
COOKIE_SECURE=true
```

The backend refuses to start when the two values disagree: an `https://` `APP_BASE_URL` requires
`COOKIE_SECURE=true`, and `COOKIE_SECURE=true` requires an `https://` `APP_BASE_URL`. This keeps
session cookies `Secure` and every generated link and OAuth redirect URI on HTTPS. `COOKIE_SECURE`
accepts only `true` or `false`.

### Legal notice and privacy policy

LILLY publishes an imprint at `/imprint` and a privacy policy at `/privacy`; both are public and linked from the footer of every page. The imprint is configured through the following variables in `/opt/lilly/shared/.env.production`, so no source change is needed. Use `|` to separate lines:

```dotenv
IMPRINT_OPERATOR_NAME=<name of the person or association>
IMPRINT_ADDRESS=<street and number>|<postal code and city>|<country>
IMPRINT_EMAIL=<contact address>
# Optional:
IMPRINT_PHONE=
IMPRINT_RESPONSIBLE_PERSON=
IMPRINT_ADDITIONAL=<register entry>|<VAT ID>
IMPRINT_LAST_UPDATED=2026-09-20
```

Name, address and email are required; until all three are set the page tells visitors that no imprint has been configured. **The operator is responsible for reviewing and completing the legal content (imprint and privacy policy) for the actual installation.** The texts shipped with LILLY are a starting point, not legal advice.

Create `/opt/lilly/shared/.deployment.env`:

```dotenv
LILLY_IMAGE_TAG=<FULL_COMMIT_SHA>
LILLY_BIND_ADDRESS=127.0.0.1
LILLY_HOST_PORT=8091
LILLY_RESOURCE_PREFIX=lilly
```

The host port stays on the loopback interface and is only used for health checks by `deploy.sh` and `restore.sh`; users reach LILLY exclusively through Traefik over HTTPS. If `.deployment.env` is missing, `deploy.sh` defaults to `127.0.0.1`.

## Routine deployment

The `Main Branch` GitHub Actions workflow performs all tests, publishes both commit-tagged images, uploads the `deploy/` directory as an immutable release and invokes:

```bash
/opt/lilly/releases/<FULL_COMMIT_SHA>/scripts/deploy.sh <FULL_COMMIT_SHA>
```

The script validates the Compose model, creates a mandatory backup when a previous release exists, pulls exact image tags, waits for all healthchecks and switches `/opt/lilly/current` only after the release is healthy.

While holding the deployment lock, it atomically installs the candidate release's
backup implementation as `/opt/lilly/shared/backup.sh`. Both the backup service
and manual backups use this stable entry point. The pre-deployment backup invokes
the same fresh helper and reuses the deployment lock inherited from `deploy.sh`.
Its configuration still comes from the previous infrastructure release, with
`current` as the bootstrap fallback before an infrastructure reference exists.

Before starting containers, it atomically points `/opt/lilly/infrastructure` at the
attempted release. This independent reference retains its Compose configuration
and recovery scripts even if the application rolls back to an older release.
`current` identifies the healthy application release; the infrastructure reference
is retained after partial failures and is recorded in the recovery information.
Do not delete any release referenced by these links or `shared/backup-tools`.

### One-time migration of an existing backup service

An already installed service may still execute `current/scripts/backup.sh`, which
can retain older, unlocked code after an application rollback. Update the loaded
unit before the first deployment using the shared backup entry point. Provisioning
a fresh server already installs the updated unit.

Pause the timer and let any running backup finish before updating the service.
Also let manually started legacy backups finish before this migration.

```bash
sudo systemctl stop lilly-backup.timer
systemctl show --property=ActiveState lilly-backup.service
# Continue when the service is inactive or failed and no old manual backup is running.
sudo install -m 0644 /opt/lilly/releases/<NEW_RELEASE_SHA>/systemd/lilly-backup.service /etc/systemd/system/
sudo install -m 0644 /opt/lilly/releases/<NEW_RELEASE_SHA>/systemd/lilly-backup-failure.service /etc/systemd/system/
sudo systemctl daemon-reload
systemctl show --property=ExecStart lilly-backup.service
systemctl show --property=OnFailure lilly-backup.service
```

The loaded `ExecStart` must point to `/opt/lilly/shared/backup.sh`. Deployment
checks this setting and refuses an outdated unit or an unreadable service state
before starting a backup or changing the database. It also refuses to continue
while a service invocation from before the helper update is still running.
The uploaded candidate release directory remains available when this preflight
check aborts a workflow; update its unit and retry that deployment.

Before this upgrade also create the private `.env.operations` from the example,
set actual alert recipients, and run the new release's `configure-operations.py
--root /opt/lilly --apply`. The loaded `OnFailure` must be
`lilly-backup-failure.service`. Deployment refuses missing operations configuration
and incomplete unit migration before changing application data. Its helper-bundle
link is published under the deployment lock and remains current after rollback.

After deployment has installed the shared helper, enable the timer again. The
helper remains updated even if application startup subsequently fails:

```bash
test -x /opt/lilly/shared/backup.sh
sudo systemctl start lilly-backup.timer
```

### MariaDB 13 upgrades

The database uses MariaDB 13.0.2, a stable rolling release. Its community support
currently ends in Q4 2026; plan the next server update according to the
[MariaDB support policy](https://mariadb.org/about/).

`MARIADB_AUTO_UPGRADE=1` lets the official image update existing system tables
before normal startup. The image retains its automatic system-database backup;
the mandatory pre-deployment backup separately protects the application database,
media and erasure ledger. The first deployment on an existing 12.3 volume therefore
performs a database-server upgrade. A fresh volume initializes directly on 13.0.

Manual status checks:

```bash
docker compose \
  --project-name lilly \
  --env-file /opt/lilly/shared/.env.production \
  --env-file /opt/lilly/shared/.deployment.env \
  --file /opt/lilly/infrastructure/docker-compose.production.yml \
  ps

curl --fail http://127.0.0.1:8091/api/v1/health
```

## Backups and restore

After the first successful deployment, install/refresh the unit files from the retained infrastructure release and enable the timer:

```bash
sudo install -m 0644 /opt/lilly/infrastructure/systemd/lilly-backup.service /etc/systemd/system/
sudo install -m 0644 /opt/lilly/infrastructure/systemd/lilly-backup.timer /etc/systemd/system/
sudo install -m 0644 /opt/lilly/infrastructure/systemd/lilly-backup-failure.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now lilly-backup.timer
sudo systemctl start lilly-backup.service
```

Inspect the result with:

```bash
systemctl status lilly-backup.timer
journalctl -u lilly-backup.service
find /opt/lilly/backups -maxdepth 2 -type f -name COMPLETE -print
```

For a manual backup, use the same maintained entry point as the service:

```bash
/opt/lilly/shared/backup.sh manual
```

Each backup records both the application release and the infrastructure
configuration release in `metadata.env`. Use the shared helper after a rollback;
an older release's `scripts/backup.sh` may not implement the current lock protocol.

Every complete backup contains `account-erasure.log` and its checksum. The live append-only ledger remains at `/opt/lilly/shared/erasure-ledger/account-erasure.log` and must never be replaced by an older backup copy. Its file mode is `0600`; the containing directory is owned by the deployment user and the backend container group (GID 999) with mode `0770`.

### Local retention and alerts

The selected operating policy is **local backups only**, daily at 03:30
`Europe/Berlin` with up to 15 minutes of random delay, retained for 14 days.
There is no configured offsite destination. Losing the VPS and its local storage
also loses these backups; recovery below assumes the files survived.

`shared/.env.operations` configures `BACKUP_DIR`, `BACKUP_RETENTION_DAYS` (1–14),
`BACKUP_ON_CALENDAR`, `BACKUP_RANDOMIZED_DELAY`, `BACKUP_ALERT_EMAILS` and
`BACKUP_ALERT_TIMEOUT_SECONDS` (1–60, default 10). It is host-only, mode `0600`.
Run `configure-operations.py --root /opt/lilly --apply` after calendar changes and
inspect `systemctl list-timers lilly-backup.timer`. A custom backup directory must
be dedicated to LILLY and writable by `lilly-deploy`; existing backups are not
silently moved when changing its location.

Rotation uses UTC timestamps recorded in snapshot names, including incomplete
sets, rather than directory modification times. Failed snapshots are removed;
expired sets are also pruned on a failed run once the locks were acquired. No
unlimited oldest-backup exception extends the personal-data retention period.
The current live ledger is outside that rotation.

The backend and its background writers are paused while database, media and
ledger are captured. This briefly interrupts API requests. A failed resume,
dump, archive or rotation fails the job and is reported. Both ordinary runs and
pre-deployment backups use the same implementation and lock order.

Alerts use the existing SMTP settings directly on the host, independently of
the database, HTTP process and Docker containers. Recipients are explicit and
deduplicated. Each unsent recipient gets two bounded attempts; accepted deliveries
are recorded to suppress repeat service notifications. SMTP failures remain in
the journal; they do not turn a backup failure into success. Alert delivery state
expires after 14 days. TLS is required outside the isolated acceptance fixture.

Test the actual recipient and TLS configuration before enabling the timer:

```bash
sudo -u lilly-deploy python3 /opt/lilly/shared/backup-tools/operations.py \
  --root /opt/lilly notify --run-id commissioning-mail-test --kind manual --phase service
sudo -u lilly-deploy /opt/lilly/shared/backup.sh manual
```

Use a new run ID when intentionally sending another test; a confirmed recipient
is deduplicated for the same ID. Inspect `journalctl -u lilly-backup.service
-u lilly-backup-failure.service` for failures before the script could start.

Version 2 snapshots include a manifest, checked configuration copies and a release
configuration archive in addition to database, media and ledger. These contain
credentials and require the same restricted access as the database dump. Before
destructive restore, the tool checks required hashes, gzip integrity and archive
paths/types; it rejects traversal, symlinks and device files. Existing snapshots
with the required ledger and hashes remain readable for an in-place restore.

Backups created before account-erasure support do not contain this marker and are intentionally rejected by `restore.sh`. After rolling out this version, create and verify a fresh backup before accepting deletion requests; retire all older restore sets according to the applicable retention policy.

A restore is intentionally manual and destructive:

```bash
/opt/lilly/infrastructure/scripts/restore.sh \
  --backup /opt/lilly/backups/<BACKUP_DIRECTORY> \
  --confirm RESTORE_LILLY
```

The restore script refuses to proceed without the live erasure ledger. After restoring database and media, it runs `lilly-backend privacy replay-erasure-ledger` before starting any public service. The replay permanently removes every account recorded after the restored snapshot. A replay failure keeps the stack offline; investigate the ledger and database instead of bypassing this guard.

On a fresh host, successful readiness also establishes the missing `current`
reference using the recovered infrastructure configuration. The restored image
tag remains unchanged. This gives the next deployment a rollback configuration
and ensures it takes its mandatory pre-deployment backup. In-place recovery keeps
the existing application reference.

Deployment, backups and restore all use `shared/.deploy.lock`. Backups and restore
then acquire `backups/.backup.lock`, always in that order. A standalone backup
acquires the deployment lock before reading configuration or inspecting containers.
A deployment-invoked backup verifies and reuses the inherited descriptor for that
same lock; it keeps the parent deployment locked after the backup finishes.
Standalone backups fail immediately when either lock is busy. Restore waits up to
15 minutes for each lock. Both backup and restore retain their locks through the
entire operation, including cleanup; restore also holds them through the final
health check. This prevents snapshots during deployment, upgrade or recovery.

Before stopping application services or changing data, restore requires one
running database container and compares its exact image reference, including the
digest, with the configured database image. A missing container, an inspection
failure or an image mismatch aborts recovery without changing the services or data.
This command restores application data on the existing server version; it does not
implicitly initialize or switch the database server.

If a deployment failed before the database switched images, the infrastructure
reference can identify the attempted upgrade while the older database is still
running. Verify the running image and select its matching infrastructure release
before retrying. If the database is stopped, first start the verified matching
server with its existing volume. Never choose an older image for an upgraded data
directory.

On installations predating this infrastructure reference, run the updated
deployment script to establish it before using the new recovery command. After a
failed database upgrade, do not invoke `current/scripts/restore.sh`: `current` can
still reference an older release whose restore script has no image guard.

Verify a backup in a disposable environment before relying on it for production recovery.

## DNS and HTTPS

1. Point the DNS A record for `lilly.maddrax-fanclub.de` to the server. Add an AAAA record only if IPv6 reachability was tested.
2. Make sure the Traefik prerequisites above are met. Traefik requests and renews the certificate automatically on the first request for the host name (HTTP-01 challenge on port 80). Test with the ACME staging CA first if the host name was never issued before, to avoid Let's Encrypt rate limits.
3. Verify HTTPS, the HTTP-to-HTTPS redirect, secure cookies, registration, password reset, `Retry-After`, client-IP handling (the backend must see the real client address, not a Docker gateway), API calls and media access.
4. Confirm that `<SERVER_IP>:8091` is not reachable from outside. Then repeat the baseline checks for the other domains served by Traefik.

## Rebuilding on a fresh server

1. Install Docker and the Traefik stack (see prerequisites) and create the `proxy` network.
2. Run `deploy/scripts/provision-server.sh` **on the empty host, before restoring any file below `/opt/lilly`**. It creates the deployment user and the empty account-erasure ledger; on a host where `/opt/lilly` already exists it deliberately refuses to create a replacement ledger.
3. Supply a surviving complete version 2 backup and the latest separately retained live ledger. From the recovery checkout run `sudo python3 deploy/scripts/operations.py --root /opt/lilly recover-files /path/to/backup --live-ledger /path/to/latest-live-ledger --owner lilly-deploy`. The helper refuses to overwrite private configuration or a nonempty ledger, restores the infrastructure release and keeps the supplied current ledger. Copy the selected backup into the configured local backup directory. Review the restored hostname, SMTP settings and deployment bindings for the new host.
4. Using `/opt/lilly/infrastructure/docker-compose.production.yml` and both restored environment files, start **only the database** with `docker compose ... up -d --wait db`; then run `/opt/lilly/infrastructure/scripts/restore.sh --backup <local-backup> --confirm RESTORE_LILLY`. The manifest records the exact database image; the restore checks it against the running server. The backend executable is invoked explicitly for ledger replay before public services start. Do not publish restored user data before that replay succeeds.
5. Update `LILLY_SERVER_KNOWN_HOSTS` if the host keys changed, re-enable `lilly-backup.timer` and verify one full backup.

The database-only start and subsequent restore in step 4 use these complete commands:

```bash
docker compose --project-name lilly \
  --env-file /opt/lilly/shared/.env.production \
  --env-file /opt/lilly/shared/.deployment.env \
  --file /opt/lilly/infrastructure/docker-compose.production.yml \
  up -d --wait db
sudo -u lilly-deploy bash /opt/lilly/infrastructure/scripts/restore.sh \
  --backup /opt/lilly/backups/<BACKUP_DIRECTORY> --confirm RESTORE_LILLY
```

Run a restore rehearsal at least monthly in an isolated environment, coordinated
by the configured operations recipients. Verify representative account/collection
records, a downloaded photo hash, and a post-snapshot account erasure against the
newest ledger. Record date, revision, backup ID and results without personal data.
The automated equivalent and the host commissioning boundaries are documented in
[operations acceptance](operations-acceptance.md).

## Health monitoring

`GET /api/v1/health` is readiness: HTTP 200 with
`{"status":"ok","database":true}` only when the database, media directory and
live ledger are usable. Probes run concurrently within a two-second budget,
including pool acquisition. Failure returns HTTP 503 with `status: "degraded"`;
`database` still describes only the database. No internal paths or diagnostic
secrets are returned. SMTP, OAuth and wiki availability do not gate readiness.

`GET /api/v1/health/live` returns HTTP 200 with `{"status":"ok"}` without
dependency checks. Use readiness for startup/deployment/restore and liveness for
a process check. Both endpoints are public and excluded from API rate limits.
Docker's `unhealthy` flag alone does not restart containers; the existing restart
policy handles exited processes. Restore reads the configured host port instead
of assuming 8091.

## Rollback

Application container rollback is automatic when a deployment healthcheck fails.
It restores only `backend`, `frontend` and `caddy` with `--no-deps`; the database
container and upgraded data directory remain at their current server version.
The independent `infrastructure` reference remains on the attempted release, so
recovery continues to use its database configuration and updated restore script.

For an application-only rollback, set the earlier application's commit tag in
`.deployment.env`, retain the infrastructure Compose configuration, and run:

```bash
docker compose \
  --project-name lilly \
  --env-file /opt/lilly/shared/.env.production \
  --env-file /opt/lilly/shared/.deployment.env \
  --file /opt/lilly/infrastructure/docker-compose.production.yml \
  up -d --wait --no-deps backend frontend caddy
```

After upgrading a database server, do not invoke an older release's deployment
script or restart its database image against the upgraded data directory. A
server-version rollback requires a fresh volume for the earlier server and the
pre-upgrade logical backup; the upgraded volume must not be reused for a downgrade.

Database migrations run at backend startup. If an old image is not compatible with the migrated schema, stop and use the pre-deployment backup rather than repeatedly restarting versions. A data restore requires explicit administrative approval and must never target another Compose project's volumes.
