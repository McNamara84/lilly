#!/usr/bin/env bash
set -Eeuo pipefail

readonly LILLY_ROOT="${LILLY_ROOT:-/opt/lilly}"
readonly SHARED_DIR="${LILLY_ROOT}/shared"
readonly APP_ENV_FILE="${SHARED_DIR}/.env.production"
readonly DEPLOYMENT_ENV_FILE="${SHARED_DIR}/.deployment.env"
readonly INFRASTRUCTURE_COMPOSE_FILE="${LILLY_ROOT}/infrastructure/docker-compose.production.yml"
readonly ERASURE_LEDGER="${SHARED_DIR}/erasure-ledger/account-erasure.log"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly OPS_HELPER="${SCRIPT_DIR}/operations.py"

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

umask 077
exec 8>"${SHARED_DIR}/.deploy.lock"
if ! flock -w 900 8; then
  echo "Another LILLY deployment or restore did not finish within 15 minutes." >&2
  exit 1
fi
operation_values="$(python3 "${OPS_HELPER}" --root "${LILLY_ROOT}" settings)"
mapfile -t settings <<<"${operation_values}"
readonly BACKUP_ROOT="${settings[0]}"
if [[ -L "${backup_dir}" ]]; then
  echo "Restore cannot use a symlinked backup directory." >&2
  exit 2
fi
backup_dir="$(realpath -- "${backup_dir}")"
backup_root_real="$(realpath -- "${BACKUP_ROOT}")"
if [[ "$(dirname -- "${backup_dir}")" != "${backup_root_real}" ]]; then
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
  "${INFRASTRUCTURE_COMPOSE_FILE}"; do
  if [[ ! -f "${required_file}" ]]; then
    echo "Required restore file is missing: ${required_file}" >&2
    exit 1
  fi
done

# Deployments take the deployment lock before invoking the pre-deployment backup.
# Use the same order and hold the backup lock through validation and recovery.
exec 9>"${BACKUP_ROOT}/.backup.lock"
if ! flock -w 900 9; then
  echo "Another LILLY backup or restore did not finish within 15 minutes." >&2
  exit 1
fi

snapshot_db_image="$(python3 "${OPS_HELPER}" --root "${LILLY_ROOT}" verify "${backup_dir}" --database-image)"

compose() {
  docker compose \
    --project-name "${LILLY_PROJECT_NAME:-lilly}" \
    --env-file "${APP_ENV_FILE}" \
    --env-file "${DEPLOYMENT_ENV_FILE}" \
    --file "${INFRASTRUCTURE_COMPOSE_FILE}" \
    "$@"
}

configured_db_image="$(compose config --images db)"
db_container="$(compose ps --status running --quiet db)"
if [[ -z "${configured_db_image}" || "${configured_db_image}" == *$'\n'* \
  || -z "${db_container}" || "${db_container}" == *$'\n'* ]]; then
  echo "Restore requires one configured database image and one running database container; refusing to start a database implicitly." >&2
  exit 1
fi
running_db_image="$(docker inspect --format '{{.Config.Image}}' "${db_container}")"
if [[ "${configured_db_image}" != "${running_db_image}" ]]; then
  echo "Configured database image differs from the running server; refusing unsafe restore." >&2
  echo "Configured: ${configured_db_image}" >&2
  echo "Running: ${running_db_image}" >&2
  echo "Select the infrastructure release matching the running database before retrying." >&2
  exit 1
fi
if [[ -n "${snapshot_db_image}" && "${snapshot_db_image}" != "${running_db_image}" ]]; then
  echo "Backup database image differs from the running server; rebuild with the recorded infrastructure release." >&2
  exit 1
fi

resource_prefix="$(sed -n 's/^LILLY_RESOURCE_PREFIX=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
resource_prefix="${resource_prefix:-lilly}"
if [[ ! "${resource_prefix}" =~ ^[a-z0-9][a-z0-9_-]*$ ]]; then
  echo "LILLY_RESOURCE_PREFIX contains unsafe characters" >&2
  exit 1
fi
readonly MEDIA_VOLUME="${resource_prefix}_media_data"
host_port="$(sed -n 's/^LILLY_HOST_PORT=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
host_port="${host_port:-8091}"
if [[ ! "${host_port}" =~ ^[0-9]+$ ]] || (( host_port < 1024 || host_port > 65535 )); then
  echo "LILLY_HOST_PORT must be an unprivileged TCP port" >&2
  exit 1
fi

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
  caddy:2.11.9-alpine@sha256:003a97808e781aa9f7e75cc2f43d34d461099031faf5e20faefb9f61126836f0 \
  sh -ec 'find /restore -mindepth 1 -delete; tar -C /restore -xzf -' \
  <"${backup_dir}/media.tar.gz"

echo "Replaying the live account-erasure ledger against the restored snapshot..."
compose run --rm --no-deps backend ./lilly-backend privacy replay-erasure-ledger

echo "Starting the restored LILLY stack..."
compose up -d --wait --remove-orphans
curl --fail --silent --show-error --max-time 5 \
  "http://127.0.0.1:${host_port}/api/v1/health" >/dev/null

# A fresh-host restore has infrastructure files but no application reference yet.
# Record its healthy configuration so the next deployment takes a mandatory
# pre-deployment backup and has a working rollback target.
if [[ ! -e "${LILLY_ROOT}/current" && ! -L "${LILLY_ROOT}/current" ]]; then
  ln -sfn "$(readlink -f -- "${LILLY_ROOT}/infrastructure")" "${LILLY_ROOT}/current.next"
  mv -Tf "${LILLY_ROOT}/current.next" "${LILLY_ROOT}/current"
fi

echo "Restore completed successfully from ${backup_dir}."
