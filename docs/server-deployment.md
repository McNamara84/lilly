# LILLY server deployment runbook

This runbook covers the production Compose stack in `deploy/`. The stack is deliberately isolated from the existing `maddrax-fanclub` and `nextcloud` Compose projects. Public HTTPS traffic is terminated by a shared Traefik reverse proxy that is operated outside this repository.

## Safety invariants

- Always pass `--project-name lilly` and the absolute LILLY Compose file.
- Never bind the LILLY stack itself to host ports 80 or 443.
- Never run a global Docker volume prune as part of deployment or backup cleanup.
- Never change the shared Traefik stack, its certificates or the labels of other Compose projects as part of a LILLY deployment.
- Never publish the LILLY host port on a public address; it is reserved for local health checks (`127.0.0.1:8091`).
- Keep `/opt/lilly/shared/.env.production` on the server only, with mode `0600`.

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
deletions. Recover the live ledger from the separately retained offsite copy instead.

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

Manual status checks:

```bash
docker compose \
  --project-name lilly \
  --env-file /opt/lilly/shared/.env.production \
  --env-file /opt/lilly/shared/.deployment.env \
  --file /opt/lilly/current/docker-compose.production.yml \
  ps

curl --fail http://127.0.0.1:8091/api/v1/health
```

## Backups and restore

After the first successful deployment, install/refresh the unit files from the active release and enable the timer:

```bash
sudo install -m 0644 /opt/lilly/current/systemd/lilly-backup.service /etc/systemd/system/
sudo install -m 0644 /opt/lilly/current/systemd/lilly-backup.timer /etc/systemd/system/
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

Every complete backup contains `account-erasure.log` and its checksum. The live append-only ledger remains at `/opt/lilly/shared/erasure-ledger/account-erasure.log`; it must be included in offsite copies and must never be replaced by an older backup copy. Its file mode is `0600`; the containing directory is owned by the deployment user and the backend container group (GID 999) with mode `0770`.

Backups created before account-erasure support do not contain this marker and are intentionally rejected by `restore.sh`. After rolling out this version, create and verify a fresh backup before accepting deletion requests; retire all older restore sets according to the applicable retention policy.

A restore is intentionally manual and destructive:

```bash
/opt/lilly/current/scripts/restore.sh \
  --backup /opt/lilly/backups/<BACKUP_DIRECTORY> \
  --confirm RESTORE_LILLY
```

The restore script refuses to proceed without the live erasure ledger. After restoring database and media, it runs `lilly-backend privacy replay-erasure-ledger` before starting any public service. The replay permanently removes every account recorded after the restored snapshot. A replay failure keeps the stack offline; investigate the ledger and database instead of bypassing this guard.

Verify a backup in a disposable environment before relying on it for production recovery.

## DNS and HTTPS

1. Point the DNS A record for `lilly.maddrax-fanclub.de` to the server. Add an AAAA record only if IPv6 reachability was tested.
2. Make sure the Traefik prerequisites above are met. Traefik requests and renews the certificate automatically on the first request for the host name (HTTP-01 challenge on port 80). Test with the ACME staging CA first if the host name was never issued before, to avoid Let's Encrypt rate limits.
3. Verify HTTPS, the HTTP-to-HTTPS redirect, secure cookies, registration, password reset, `Retry-After`, client-IP handling (the backend must see the real client address, not a Docker gateway), API calls and media access.
4. Confirm that `<SERVER_IP>:8091` is not reachable from outside. Then repeat the baseline checks for the other domains served by Traefik.

## Rebuilding on a fresh server

1. Install Docker and the Traefik stack (see prerequisites) and create the `proxy` network.
2. Run `deploy/scripts/provision-server.sh` **on the empty host, before restoring any file below `/opt/lilly`**. It creates the deployment user and the empty account-erasure ledger; on a host where `/opt/lilly` already exists it deliberately refuses to create a replacement ledger.
3. Restore `/opt/lilly/shared/.env.production` and `/opt/lilly/shared/.deployment.env` from the offsite copy, the backup directories below `/opt/lilly/backups/` and, if one exists, the live ledger `/opt/lilly/shared/erasure-ledger/account-erasure.log`. Never restore the ledger from an older snapshot than the newest available copy.
4. Deploy the release (`workflow_dispatch` of the `Main Branch` workflow, or `deploy.sh <FULL_COMMIT_SHA>`) and then run `restore.sh` with the newest complete backup.
5. Update `LILLY_SERVER_KNOWN_HOSTS` if the host keys changed, re-enable `lilly-backup.timer` and verify one full backup.

## Rollback

Container rollback is automatic when a deployment healthcheck fails. To select an older release manually, restore its commit tag in `.deployment.env` and run that release's `deploy.sh`.

Database migrations run at backend startup. If an old image is not compatible with the migrated schema, stop and use the pre-deployment backup rather than repeatedly restarting versions. A data restore requires explicit administrative approval and must never target another Compose project's volumes.
