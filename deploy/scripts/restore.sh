#!/usr/bin/env bash
set -Eeuo pipefail

readonly LILLY_ROOT="${LILLY_ROOT:-/opt/lilly}"
readonly BACKUP_ROOT="${LILLY_ROOT}/backups"
readonly SHARED_DIR="${LILLY_ROOT}/shared"
readonly APP_ENV_FILE="${SHARED_DIR}/.env.production"
readonly DEPLOYMENT_ENV_FILE="${SHARED_DIR}/.deployment.env"
readonly CURRENT_COMPOSE_FILE="${LILLY_ROOT}/current/docker-compose.production.yml"
readonly ERASURE_LEDGER="${SHARED_DIR}/erasure-ledger/account-erasure.log"

backup_dir=""
confirmation=""
while (( $# > 0 )); do
  case "$1" in
    --backup)
      backup_dir="${2:-}"
      shift 2
      ;;
    --confirm)
      confirmation="${2:-}"
      shift 2
      ;;
    *)
      echo "Usage: $0 --backup /opt/lilly/backups/<backup> --confirm RESTORE_LILLY" >&2
      exit 2
      ;;
  esac
done

if [[ ! -f "${ERASURE_LEDGER}" ]]; then
  echo "Live account-erasure ledger is missing; refusing unsafe restore: ${ERASURE_LEDGER}" >&2
  exit 1
fi

if [[ "${confirmation}" != "RESTORE_LILLY" || -z "${backup_dir}" ]]; then
  echo "Restore requires --backup and --confirm RESTORE_LILLY" >&2
  exit 2
fi

backup_dir="$(realpath -- "${backup_dir}")"
backup_root_real="$(realpath -- "${BACKUP_ROOT}")"
if [[ "${backup_dir}" != "${backup_root_real}/"* ]]; then
  echo "Backup must be located directly below ${BACKUP_ROOT}" >&2
  exit 2
fi

for required_file in \
  "${backup_dir}/COMPLETE" \
  "${backup_dir}/SHA256SUMS" \
  "${backup_dir}/database.sql.gz" \
  "${backup_dir}/media.tar.gz" \
  "${backup_dir}/account-erasure.log" \
  "${APP_ENV_FILE}" \
  "${DEPLOYMENT_ENV_FILE}" \
  "${CURRENT_COMPOSE_FILE}"; do
  if [[ ! -f "${required_file}" ]]; then
    echo "Required restore file is missing: ${required_file}" >&2
    exit 1
  fi
done

(
  cd "${backup_dir}"
  sha256sum --check SHA256SUMS
)

compose() {
  docker compose \
    --project-name lilly \
    --env-file "${APP_ENV_FILE}" \
    --env-file "${DEPLOYMENT_ENV_FILE}" \
    --file "${CURRENT_COMPOSE_FILE}" \
    "$@"
}

resource_prefix="$(sed -n 's/^LILLY_RESOURCE_PREFIX=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
resource_prefix="${resource_prefix:-lilly}"
if [[ ! "${resource_prefix}" =~ ^[a-z0-9][a-z0-9_-]*$ ]]; then
  echo "LILLY_RESOURCE_PREFIX contains unsafe characters" >&2
  exit 1
fi
readonly MEDIA_VOLUME="${resource_prefix}_media_data"

echo "Stopping LILLY application services..."
compose stop caddy frontend backend
compose up -d --wait db

echo "Replacing the LILLY database..."
# Expanded by the shell inside the database container, not by this script.
# shellcheck disable=SC2016
compose exec -T db sh -ec '
  case "$MARIADB_DATABASE" in
    (*[!A-Za-z0-9_]*) echo "Unsafe database name" >&2; exit 1 ;;
  esac
  mariadb --user=root --password="$MARIADB_ROOT_PASSWORD" \
    --execute="DROP DATABASE IF EXISTS \`$MARIADB_DATABASE\`; CREATE DATABASE \`$MARIADB_DATABASE\` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;"
'
# Expanded by the shell inside the database container, not by this script.
# shellcheck disable=SC2016
gzip --decompress --stdout "${backup_dir}/database.sql.gz" \
  | compose exec -T db sh -ec \
    'exec mariadb --user=root --password="$MARIADB_ROOT_PASSWORD" "$MARIADB_DATABASE"'

echo "Replacing the LILLY media volume..."
docker run --rm --interactive \
  --volume "${MEDIA_VOLUME}:/restore" \
  caddy:2.11.4-alpine \
  sh -ec 'find /restore -mindepth 1 -delete; tar -C /restore -xzf -' \
  <"${backup_dir}/media.tar.gz"

echo "Replaying the live account-erasure ledger against the restored snapshot..."
compose run --rm --no-deps backend privacy replay-erasure-ledger

echo "Starting the restored LILLY stack..."
compose up -d --wait --remove-orphans
curl --fail --silent --show-error --max-time 5 \
  http://127.0.0.1:8091/api/v1/health >/dev/null

echo "Restore completed successfully from ${backup_dir}."
