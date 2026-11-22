#!/usr/bin/env bash
set -Eeuo pipefail

readonly LILLY_ROOT="${LILLY_ROOT:-/opt/lilly}"
readonly BACKUP_KIND="${1:-manual}"
readonly SHARED_DIR="${LILLY_ROOT}/shared"
readonly APP_ENV_FILE="${SHARED_DIR}/.env.production"
readonly DEPLOYMENT_ENV_FILE="${SHARED_DIR}/.deployment.env"
readonly ERASURE_LEDGER="${SHARED_DIR}/erasure-ledger/account-erasure.log"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
OPS_HELPER="${SCRIPT_DIR}/operations.py"
if [[ ! -f "${OPS_HELPER}" ]]; then
  OPS_HELPER="${SHARED_DIR}/backup-tools/operations.py"
fi
readonly OPS_HELPER
readonly RUN_ID="${INVOCATION_ID:-$(python3 -c 'import uuid; print(uuid.uuid4().hex)')}"
if [[ ! "${RUN_ID}" =~ ^[a-f0-9]{32}$ ]]; then
  echo "INVOCATION_ID must be a 32-character hexadecimal identifier." >&2
  exit 2
fi
phase=configuration
backend_was_paused=0
rotation_ready=0
temporary_dir=""
umask 077

operations() { python3 "${OPS_HELPER}" --root "${LILLY_ROOT}" "$@"; }

finish() {
  local result=$?
  trap - EXIT
  if (( backend_was_paused == 1 )); then
    if ! compose unpause backend >/dev/null; then
      echo "Backup could not resume the backend." >&2
      phase=resume
      result=1
    fi
  fi
  if (( rotation_ready == 1 )); then
    if ! operations rotate; then
      phase=rotation
      result=1
    fi
  fi
  if [[ -n "${temporary_dir}" && -d "${temporary_dir}" && ! -L "${temporary_dir}" ]]; then
    if [[ "${temporary_dir}" == "${BACKUP_ROOT}/."*.incomplete ]]; then
      rm -rf -- "${temporary_dir}" || result=1
    fi
  fi
  if (( result != 0 )); then
    operations notify --run-id "${RUN_ID}" --kind "${BACKUP_KIND}" --phase "${phase}" || {
      echo "Backup failed; email notification also failed. Inspect the host journal." >&2
    }
  fi
  exit "${result}"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP

case "${BACKUP_KIND}" in
  manual|daily|pre-deploy) ;;
  *)
    echo "Backup kind must be manual, daily, or pre-deploy" >&2
    exit 2
    ;;
esac

for required_command in docker flock gzip python3; do
  command -v "${required_command}" >/dev/null || {
    echo "Required command is missing: ${required_command}" >&2
    exit 1
  }
done

# Reuse deploy.sh's inherited open file description rather than locking it again
# through a separate descriptor. Standalone backups open and acquire the same lock.
phase=lock
if [[ ! "${SHARED_DIR}/.deploy.lock" -ef "/proc/$$/fd/8" ]]; then
  exec 8>"${SHARED_DIR}/.deploy.lock"
fi
if ! flock -n 8; then
  echo "Another LILLY deployment, backup or restore is already running." >&2
  exit 1
fi
phase=configuration
operation_values="$(operations settings)"
mapfile -t settings <<<"${operation_values}"
readonly BACKUP_ROOT="${settings[0]}"
mkdir -p "${BACKUP_ROOT}"
chmod 700 "${BACKUP_ROOT}"
phase=lock
exec 9>"${BACKUP_ROOT}/.backup.lock"
if ! flock -n 9; then
  echo "Another LILLY backup or restore is already running." >&2
  exit 1
fi
rotation_ready=1
phase=configuration

# The current release is only a bootstrap fallback before the first deployment
# that records infrastructure independently of the application release.
if [[ -e "${LILLY_ROOT}/infrastructure" || -L "${LILLY_ROOT}/infrastructure" ]]; then
  readonly BACKUP_COMPOSE_FILE="${LILLY_ROOT}/infrastructure/docker-compose.production.yml"
else
  readonly BACKUP_COMPOSE_FILE="${LILLY_ROOT}/current/docker-compose.production.yml"
fi

for required_file in \
  "${APP_ENV_FILE}" \
  "${DEPLOYMENT_ENV_FILE}" \
  "${BACKUP_COMPOSE_FILE}"; do
  if [[ ! -f "${required_file}" ]]; then
    echo "Required backup file is missing: ${required_file}" >&2
    exit 1
  fi
done

ledger_available=1
if [[ ! -f "${ERASURE_LEDGER}" ]]; then
  if [[ "${BACKUP_KIND}" == "pre-deploy" ]] \
    && ! grep -q 'ACCOUNT_ERASURE_LEDGER_PATH' "${BACKUP_COMPOSE_FILE}"; then
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
    --project-name "${LILLY_PROJECT_NAME:-lilly}" \
    --env-file "${APP_ENV_FILE}" \
    --env-file "${DEPLOYMENT_ENV_FILE}" \
    --file "${BACKUP_COMPOSE_FILE}" \
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

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
backup_name="${timestamp}-${RUN_ID:0:8}-${BACKUP_KIND}"
temporary_dir="${BACKUP_ROOT}/.${backup_name}.incomplete"
final_dir="${BACKUP_ROOT}/${backup_name}"

if [[ -e "${temporary_dir}" || -e "${final_dir}" ]]; then
  echo "Backup path already exists: ${backup_name}" >&2
  exit 1
fi

mkdir "${temporary_dir}"
resume_backend() {
  if (( backend_was_paused == 1 )); then
    compose unpause backend >/dev/null
    backend_was_paused=0
  fi
}

backend_container="$(compose ps --quiet backend)"
if [[ -n "${backend_container}" \
  && "$(docker inspect --format '{{.State.Running}}' "${backend_container}")" == "true" \
  && "$(docker inspect --format '{{.State.Paused}}' "${backend_container}")" == "false" ]]; then
  compose pause backend >/dev/null
  backend_was_paused=1
fi

echo "Creating MariaDB dump..."
phase=database
# Expanded by the shell inside the database container, not by this script.
# shellcheck disable=SC2016
compose exec -T db sh -ec \
  'exec mariadb-dump --single-transaction --quick --skip-lock-tables --user="$MARIADB_USER" --password="$MARIADB_PASSWORD" "$MARIADB_DATABASE"' \
  | gzip -9 >"${temporary_dir}/database.sql.gz"

echo "Archiving LILLY media..."
phase=media
docker run --rm \
  --volume "${MEDIA_VOLUME}:/source:ro" \
  caddy:2.11.9-alpine@sha256:003a97808e781aa9f7e75cc2f43d34d461099031faf5e20faefb9f61126836f0 \
  tar -C /source -czf - . >"${temporary_dir}/media.tar.gz"

if (( ledger_available == 1 )); then
  phase=ledger
  # Restore keeps the newer live ledger instead of replacing it with this snapshot.
  docker run --rm \
    --volume "${LILLY_LEDGER_PATH:-${SHARED_DIR}/erasure-ledger}:/ledger:ro" \
    caddy:2.11.9-alpine@sha256:003a97808e781aa9f7e75cc2f43d34d461099031faf5e20faefb9f61126836f0 \
    cat /ledger/account-erasure.log >"${temporary_dir}/account-erasure.log"
fi

phase=resume
resume_backend
phase=snapshot

active_release="$(readlink -f -- "${LILLY_ROOT}/current")"
image_tag="$(sed -n 's/^LILLY_IMAGE_TAG=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
{
  printf 'created_utc=%s\n' "${timestamp}"
  printf 'kind=%s\n' "${BACKUP_KIND}"
  printf 'release=%s\n' "${active_release}"
  printf 'infrastructure_release=%s\n' "$(dirname -- "$(readlink -f -- "${BACKUP_COMPOSE_FILE}")")"
  printf 'image_tag=%s\n' "${image_tag}"
} >"${temporary_dir}/metadata.env"

if (( ledger_available != 1 )); then
  echo "A recoverable backup requires the live erasure ledger." >&2
  exit 1
fi
operations finish "${temporary_dir}" "${BACKUP_COMPOSE_FILE}"
touch "${temporary_dir}/COMPLETE"
chmod -R go-rwx "${temporary_dir}"
mv -- "${temporary_dir}" "${final_dir}"
temporary_dir=""
phase=rotation
operations rotate
rotation_ready=0
echo "Backup completed: ${final_dir}"
