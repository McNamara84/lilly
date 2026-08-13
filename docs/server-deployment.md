# LILLY server deployment runbook

This runbook covers the production Compose stack in `deploy/`. The stack is deliberately isolated from the existing `maddrax-fanclub` and `nextcloud` Compose projects.

## Safety invariants

- Always pass `--project-name lilly` and the absolute LILLY Compose file.
- Never bind the LILLY stack itself to host ports 80 or 443.
- Never run a global Docker volume prune as part of deployment or backup cleanup.
- Never edit the existing Nginx site files for the club website or Nextcloud.
- Run `nginx -t` before every Nginx reload.
- Keep `/opt/lilly/shared/.env.production` on the server only, with mode `0600`.

## One-time GitHub configuration

Create a GitHub Environment named `production` without a required reviewer so pushes to `main` remain automatic. Add these Environment secrets:

| Secret                     | Value                                                      |
| -------------------------- | ---------------------------------------------------------- |
| `LILLY_SERVER_HOST`        | Server hostname or IP used for SSH                         |
| `LILLY_SERVER_USER`        | `lilly-deploy`                                             |
| `LILLY_SERVER_SSH_KEY`     | Dedicated Ed25519 private key                              |
| `LILLY_SERVER_KNOWN_HOSTS` | Trusted complete OpenSSH `known_hosts` line for the server |

After the first workflow image build, ensure both `lilly-backend` and `lilly-frontend` packages are linked to this repository and publicly readable in GHCR. The deploy job uses its ephemeral repository token for the first pull and logs out immediately afterward, so no long-lived package credential is stored on the server. Do not place application, MariaDB, JWT, SMTP, or OAuth secrets in GitHub.

## One-time server provisioning

Generate a dedicated Ed25519 key, transfer only its public part with the existing administrative account, and run:

```bash
sudo deploy/scripts/provision-server.sh /path/to/lilly-deploy.pub
```

Generate the private environment with the supplied helper. It creates unique URL-safe MariaDB passwords and a high-entropy JWT secret, writes the file atomically, and sets mode `0600` and ownership to `lilly-deploy`:

```bash
sudo /opt/lilly/releases/<RELEASE>/scripts/configure-environment.py \
  --mail-source /root/maddrax-fanclub/.env.production \
  --output /opt/lilly/shared/.env.production \
  --app-base-url http://<SERVER_IP>:8091
```

Copy only the SMTP host, port, username, password and suitable sender from the club website's server-side environment into the LILLY environment. Map an existing implicit SSL/TLS setup on port 465 to `SMTP_TLS_MODE=tls`; use `SMTP_TLS_MODE=starttls` for a submission server on port 587. Do not source the other application's file at LILLY runtime and do not copy any unrelated value. Leave OAuth values and `ADMIN_EMAIL` empty until their intended values are available.

The generated environment also sets a one-hour password-reset lifetime, the documented rate-limit
defaults and `TRUSTED_PROXY_CIDRS=172.16.0.0/12` for the private Docker bridge. Do not add public
client networks to that trust list. The public Nginx configuration overwrites incoming
`X-Forwarded-For`; Caddy and the backend then parse the chain from right to left. If the Docker
network changes to a non-matching subnet, update the CIDR narrowly before enabling public traffic.

For the temporary test phase, use:

```dotenv
APP_BASE_URL=http://<SERVER_IP>:8091
COOKIE_SECURE=false
```

Create `/opt/lilly/shared/.deployment.env`:

```dotenv
LILLY_IMAGE_TAG=<FULL_COMMIT_SHA>
LILLY_BIND_ADDRESS=0.0.0.0
LILLY_HOST_PORT=8091
LILLY_RESOURCE_PREFIX=lilly
```

The public port is plain HTTP. Use only a disposable password and no sensitive personal data until HTTPS is active.

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

A restore is intentionally manual and destructive:

```bash
/opt/lilly/current/scripts/restore.sh \
  --backup /opt/lilly/backups/<BACKUP_DIRECTORY> \
  --confirm RESTORE_LILLY
```

Verify a backup in a disposable environment before relying on it for production recovery.

## DNS and HTTPS cutover

1. Point the DNS A record for `lilly.maddrax-fanclub.de` to the server. Add an AAAA record only if IPv6 reachability was tested.
2. Record baseline responses for `maddrax-fanclub.de`, `www.maddrax-fanclub.de` and `cloud.maddrax-fanclub.de`.
3. Install `deploy/nginx/lilly-http.conf` as a new LILLY-only site, create `/var/www/letsencrypt`, enable only that site, run `nginx -t`, then reload Nginx.
4. Request a separate certificate:

   ```bash
   sudo certbot certonly --webroot \
     --webroot-path /var/www/letsencrypt \
     --domain lilly.maddrax-fanclub.de
   ```

5. Replace only the new LILLY site with `deploy/nginx/lilly-https.conf`, run `nginx -t`, and reload Nginx.
6. Change the server-side values to:

   ```dotenv
   APP_BASE_URL=https://lilly.maddrax-fanclub.de
   COOKIE_SECURE=true
   LILLY_BIND_ADDRESS=127.0.0.1
   ```

7. Recreate only the LILLY stack and verify HTTPS, secure cookies, registration, password reset,
   `Retry-After`, client-IP handling, API calls and media access.
8. Confirm that public access to `<SERVER_IP>:8091` is closed and repeat all baseline checks for the existing domains.
9. Run `certbot renew --dry-run`.

## Rollback

Container rollback is automatic when a deployment healthcheck fails. To select an older release manually, restore its commit tag in `.deployment.env` and run that release's `deploy.sh`.

Database migrations run at backend startup. If an old image is not compatible with the migrated schema, stop and use the pre-deployment backup rather than repeatedly restarting versions. A data restore requires explicit administrative approval and must never target another Compose project's volumes.
