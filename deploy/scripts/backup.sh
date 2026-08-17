#!/usr/bin/env bash
set -Eeuo pipefail

readonly LILLY_ROOT="${LILLY_ROOT:-/opt/lilly}"
readonly BACKUP_KIND="${1:-manual}"
readonly BACKUP_ROOT="${LILLY_ROOT}/backups"
readonly SHARED_DIR="${LILLY_ROOT}/shared"
readonly APP_ENV_FILE="${SHARED_DIR}/.env.production"
readonly DEPLOYMENT_ENV_FILE="${SHARED_DIR}/.deployment.env"
readonly CURRENT_COMPOSE_FILE="${LILLY_ROOT}/current/docker-compose.production.yml"
readonly ERASURE_LEDGER="${SHARED_DIR}/erasure-ledger/account-erasure.log"

case "${BACKUP_KIND}" in
  manual|daily|pre-deploy) ;;
  *)
    echo "Backup kind must be manual, daily, or pre-deploy" >&2
    exit 2
    ;;
esac

for required_command in docker flock gzip sha256sum; do
  command -v "${required_command}" >/dev/null || {
    echo "Required command is missing: ${required_command}" >&2
    exit 1
  }
done

for required_file in \
  "${APP_ENV_FILE}" \
  "${DEPLOYMENT_ENV_FILE}" \
  "${CURRENT_COMPOSE_FILE}"; do
  if [[ ! -f "${required_file}" ]]; then
    if [[ "${BACKUP_KIND}" == "pre-deploy" ]]; then
      echo "No active LILLY installation exists; no backup is required."
      exit 0
    fi
    echo "Required backup file is missing: ${required_file}" >&2
    exit 1
  fi
done

ledger_available=1
if [[ ! -f "${ERASURE_LEDGER}" ]]; then
  if [[ "${BACKUP_KIND}" == "pre-deploy" ]] \
    && ! grep -q 'ACCOUNT_ERASURE_LEDGER_PATH' "${CURRENT_COMPOSE_FILE}"; then
    # A backup of the legacy release is still useful for an immediate rollout
    # rollback, but intentionally lacks the marker required by the new restore
    # path and must be retired before accepting deletion requests.
    ledger_available=0
  else
    echo "Required backup file is missing: ${ERASURE_LEDGER}" >&2
    exit 1
  fi
fi

compose() {
  docker compose \
    --project-name lilly \
    --env-file "${APP_ENV_FILE}" \
    --env-file "${DEPLOYMENT_ENV_FILE}" \
    --file "${CURRENT_COMPOSE_FILE}" \
    "$@"
}

db_container="$(compose ps --quiet db)"
if [[ -z "${db_container}" || "$(docker inspect --format '{{.State.Running}}' "${db_container}")" != "true" ]]; then
  echo "LILLY database is not running; refusing to create an incomplete backup." >&2
  exit 1
fi

resource_prefix="$(sed -n 's/^LILLY_RESOURCE_PREFIX=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
resource_prefix="${resource_prefix:-lilly}"
if [[ ! "${resource_prefix}" =~ ^[a-z0-9][a-z0-9_-]*$ ]]; then
  echo "LILLY_RESOURCE_PREFIX contains unsafe characters" >&2
  exit 1
fi
readonly MEDIA_VOLUME="${resource_prefix}_media_data"

umask 077
mkdir -p "${BACKUP_ROOT}"
exec 9>"${BACKUP_ROOT}/.backup.lock"
if ! flock -n 9; then
  echo "Another LILLY backup is already running." >&2
  exit 1
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup_name="${timestamp}-${BACKUP_KIND}"
temporary_dir="${BACKUP_ROOT}/.${backup_name}.incomplete"
final_dir="${BACKUP_ROOT}/${backup_name}"

if [[ -e "${temporary_dir}" || -e "${final_dir}" ]]; then
  echo "Backup path already exists: ${backup_name}" >&2
  exit 1
fi

mkdir "${temporary_dir}"
backend_was_paused=0

resume_backend() {
  if (( backend_was_paused == 1 )); then
    compose unpause backend >/dev/null || true
  fi
}
trap resume_backend EXIT

backend_container="$(compose ps --quiet backend)"
if [[ -n "${backend_container}" \
  && "$(docker inspect --format '{{.State.Running}}' "${backend_container}")" == "true" \
  && "$(docker inspect --format '{{.State.Paused}}' "${backend_container}")" == "false" ]]; then
  compose pause backend >/dev/null
  backend_was_paused=1
fi

echo "Creating MariaDB dump..."
# Expanded by the shell inside the database container, not by this script.
# shellcheck disable=SC2016
compose exec -T db sh -ec \
  'exec mariadb-dump --single-transaction --quick --skip-lock-tables --user="$MARIADB_USER" --password="$MARIADB_PASSWORD" "$MARIADB_DATABASE"' \
  | gzip -9 >"${temporary_dir}/database.sql.gz"

echo "Archiving LILLY media..."
docker run --rm \
  --volume "${MEDIA_VOLUME}:/source:ro" \
  caddy:2.11.4-alpine \
  tar -C /source -czf - . >"${temporary_dir}/media.tar.gz"

if (( ledger_available == 1 )); then
  # Retain a protected copy off site. Restore deliberately keeps and replays the
  # newer live ledger instead of replacing it with this point-in-time copy.
  docker run --rm \
    --volume "${SHARED_DIR}/erasure-ledger:/ledger:ro" \
    caddy:2.11.4-alpine \
    cat /ledger/account-erasure.log >"${temporary_dir}/account-erasure.log"
fi

resume_backend
backend_was_paused=0
trap - EXIT

active_release="$(readlink -f -- "${LILLY_ROOT}/current")"
image_tag="$(sed -n 's/^LILLY_IMAGE_TAG=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
{
  printf 'created_utc=%s\n' "${timestamp}"
  printf 'kind=%s\n' "${BACKUP_KIND}"
  printf 'release=%s\n' "${active_release}"
  printf 'image_tag=%s\n' "${image_tag}"
} >"${temporary_dir}/metadata.env"

(
  cd "${temporary_dir}"
  checksum_files=(database.sql.gz media.tar.gz metadata.env)
  if (( ledger_available == 1 )); then
    checksum_files+=(account-erasure.log)
  fi
  sha256sum "${checksum_files[@]}" >SHA256SUMS
)
touch "${temporary_dir}/COMPLETE"
chmod -R go-rwx "${temporary_dir}"
mv -- "${temporary_dir}" "${final_dir}"

# Retention is deliberately constrained to successful LILLY backup directories.
find "${BACKUP_ROOT}" \
  -mindepth 1 -maxdepth 1 -type d \
  -name '????????T??????Z-*' -mtime +13 \
  -exec rm -rf -- {} +

echo "Backup completed: ${final_dir}"
