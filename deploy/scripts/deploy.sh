#!/usr/bin/env bash
set -Eeuo pipefail

readonly LILLY_ROOT="${LILLY_ROOT:-/opt/lilly}"
readonly RELEASE_ID="${1:-}"
readonly SHARED_DIR="${LILLY_ROOT}/shared"
readonly RELEASE_DIR="${LILLY_ROOT}/releases/${RELEASE_ID}"
readonly APP_ENV_FILE="${SHARED_DIR}/.env.production"
readonly DEPLOYMENT_ENV_FILE="${SHARED_DIR}/.deployment.env"
readonly CANDIDATE_ENV_FILE="${SHARED_DIR}/.deployment.env.candidate"
readonly COMPOSE_FILE="${RELEASE_DIR}/docker-compose.production.yml"
readonly INFRASTRUCTURE_LINK="${LILLY_ROOT}/infrastructure"
readonly BACKUP_SCRIPT="${SHARED_DIR}/backup.sh"

if [[ ! "${RELEASE_ID}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Usage: $0 <40-character commit SHA>" >&2
  exit 2
fi

for required_file in \
  "${APP_ENV_FILE}" \
  "${SHARED_DIR}/.env.operations" \
  "${COMPOSE_FILE}" \
  "${RELEASE_DIR}/Caddyfile" \
  "${RELEASE_DIR}/scripts/backup.sh" \
  "${RELEASE_DIR}/scripts/operations.py"; do
  if [[ ! -f "${required_file}" ]]; then
    echo "Required deployment file is missing: ${required_file}" >&2
    exit 1
  fi
done

umask 077
mkdir -p "${SHARED_DIR}"
exec 8>"${SHARED_DIR}/.deploy.lock"
if ! flock -w 900 8; then
  echo "Another LILLY deployment did not finish within 15 minutes." >&2
  exit 1
fi

previous_release=""
if [[ -L "${LILLY_ROOT}/current" ]]; then
  previous_release="$(readlink -f -- "${LILLY_ROOT}/current")"
fi

previous_env_file="$(mktemp "${SHARED_DIR}/.deployment.env.previous.XXXXXX")"
had_previous_env=0
if [[ -f "${DEPLOYMENT_ENV_FILE}" ]]; then
  cp -- "${DEPLOYMENT_ENV_FILE}" "${previous_env_file}"
  had_previous_env=1
fi

cleanup() {
  rm -f -- "${CANDIDATE_ENV_FILE}" "${previous_env_file}"
}
trap cleanup EXIT

bind_address="127.0.0.1"
host_port="8091"
resource_prefix="lilly"
if [[ -f "${DEPLOYMENT_ENV_FILE}" ]]; then
  configured_bind_address="$(sed -n 's/^LILLY_BIND_ADDRESS=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
  configured_host_port="$(sed -n 's/^LILLY_HOST_PORT=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
  configured_resource_prefix="$(sed -n 's/^LILLY_RESOURCE_PREFIX=//p' "${DEPLOYMENT_ENV_FILE}" | tail -n 1)"
  if [[ -n "${configured_bind_address}" ]]; then
    bind_address="${configured_bind_address}"
  fi
  if [[ -n "${configured_host_port}" ]]; then
    host_port="${configured_host_port}"
  fi
  if [[ -n "${configured_resource_prefix}" ]]; then
    resource_prefix="${configured_resource_prefix}"
  fi
fi

if [[ "${bind_address}" != "127.0.0.1" ]]; then
  echo "LILLY_BIND_ADDRESS must be 127.0.0.1; production ingress belongs to Traefik." >&2
  exit 1
fi
if [[ ! "${host_port}" =~ ^[0-9]+$ ]] || (( host_port < 1024 || host_port > 65535 )); then
  echo "LILLY_HOST_PORT must be an unprivileged TCP port" >&2
  exit 1
fi
if [[ ! "${resource_prefix}" =~ ^[a-z0-9][a-z0-9_-]*$ ]]; then
  echo "LILLY_RESOURCE_PREFIX contains unsafe characters" >&2
  exit 1
fi

printf 'LILLY_IMAGE_TAG=%s\nLILLY_BIND_ADDRESS=%s\nLILLY_HOST_PORT=%s\nLILLY_RESOURCE_PREFIX=%s\n' \
  "${RELEASE_ID}" "${bind_address}" "${host_port}" "${resource_prefix}" \
  >"${CANDIDATE_ENV_FILE}"
chmod 600 "${CANDIDATE_ENV_FILE}"

compose_new() {
  docker compose \
    --project-name "${LILLY_PROJECT_NAME:-lilly}" \
    --env-file "${APP_ENV_FILE}" \
    --env-file "${CANDIDATE_ENV_FILE}" \
    --file "${COMPOSE_FILE}" \
    "$@"
}

rollback() {
  echo "Deployment failed; attempting application container rollback." >&2
  echo "The database image and data directory will remain at their current version." >&2
  echo "Recovery tooling: ${INFRASTRUCTURE_LINK}/scripts/restore.sh" >&2

  if (( had_previous_env == 1 )); then
    install -m 600 "${previous_env_file}" "${DEPLOYMENT_ENV_FILE}"
  else
    rm -f -- "${DEPLOYMENT_ENV_FILE}"
  fi

  if [[ -n "${previous_release}" && -f "${previous_release}/docker-compose.production.yml" && -f "${DEPLOYMENT_ENV_FILE}" ]]; then
    if ! docker compose \
      --project-name "${LILLY_PROJECT_NAME:-lilly}" \
      --env-file "${APP_ENV_FILE}" \
      --env-file "${DEPLOYMENT_ENV_FILE}" \
      --file "${previous_release}/docker-compose.production.yml" \
      up -d --wait --no-deps backend frontend caddy; then
      echo "Automatic application rollback also failed. Use ${INFRASTRUCTURE_LINK}/scripts/restore.sh with the pre-deployment backup for recovery." >&2
    fi
  else
    compose_new stop || true
  fi
}

echo "Validating release ${RELEASE_ID}..."
compose_new config --quiet

if ! docker network inspect "${LILLY_PROXY_NETWORK:-proxy}" >/dev/null 2>&1; then
  echo "The external Docker network 'proxy' does not exist. Start the Traefik stack first." >&2
  exit 1
fi

# An installed legacy unit could keep executing unlocked code after an application
# rollback. Inspect the loaded unit, including overrides, before changing the stack.
backup_service_state="$(systemctl show --property=LoadState --value lilly-backup.service)" || {
  if [[ "${backup_service_state}" != "not-found" ]]; then
    echo "Cannot inspect lilly-backup.service; refusing deployment." >&2
    exit 1
  fi
}
if [[ "${backup_service_state}" == "loaded" ]]; then
  backup_service_exec="$(systemctl show --property=ExecStart --value lilly-backup.service)"
  if [[ "${backup_service_exec}" != *"path=${BACKUP_SCRIPT} ;"* ]]; then
    echo "Update lilly-backup.service from ${RELEASE_DIR}/systemd/ and run systemctl daemon-reload before deploying." >&2
    exit 1
  fi
  backup_failure_unit="$(systemctl show --property=OnFailure --value lilly-backup.service)"
  if [[ "${backup_failure_unit}" != "lilly-backup-failure.service" ]]; then
    echo "Update lilly-backup.service and lilly-backup-failure.service before deploying." >&2
    exit 1
  fi
elif [[ "${backup_service_state}" != "not-found" ]]; then
  echo "Unexpected lilly-backup.service load state: ${backup_service_state}" >&2
  exit 1
fi

if [[ "${backup_service_state}" == "loaded" ]]; then
  backup_service_active="$(systemctl show --property=ActiveState --value lilly-backup.service)"
  if [[ "${backup_service_active}" != "inactive" && "${backup_service_active}" != "failed" ]]; then
    echo "The backup service is still running; let it finish before retrying deployment." >&2
    exit 1
  fi
fi

# Validate before changing recovery tools or application data. Publish the complete
# helper bundle under the deployment lock; application rollback never rewinds it.
python3 "${RELEASE_DIR}/scripts/operations.py" --root "${LILLY_ROOT}" settings >/dev/null
ln -sfn "${RELEASE_DIR}/scripts" "${SHARED_DIR}/backup-tools.next"
mv -Tf "${SHARED_DIR}/backup-tools.next" "${SHARED_DIR}/backup-tools"
install -m 700 "${RELEASE_DIR}/scripts/backup.sh" "${BACKUP_SCRIPT}.next"
mv -Tf "${BACKUP_SCRIPT}.next" "${BACKUP_SCRIPT}"

if [[ -n "${previous_release}" ]]; then
  "${BACKUP_SCRIPT}" pre-deploy
else
  echo "No active LILLY release exists yet; skipping the initial pre-deployment backup."
fi

echo "Pulling immutable release images..."
compose_new pull --policy missing

# Record recovery tooling and infrastructure before the database can be upgraded.
# Application rollback deliberately leaves this reference on the attempted release.
ln -sfn "${RELEASE_DIR}" "${INFRASTRUCTURE_LINK}.next"
mv -Tf "${INFRASTRUCTURE_LINK}.next" "${INFRASTRUCTURE_LINK}"

install -m 600 "${CANDIDATE_ENV_FILE}" "${DEPLOYMENT_ENV_FILE}"

echo "Starting LILLY release ${RELEASE_ID}..."
if ! compose_new up -d --wait --remove-orphans; then
  rollback
  exit 1
fi

health_ok=0
for attempt in $(seq 1 30); do
  if curl --fail --silent --show-error \
    --max-time 5 \
    "http://127.0.0.1:${host_port}/api/v1/health" >/dev/null \
    && curl --fail --silent --show-error \
      --max-time 5 \
      "http://127.0.0.1:${host_port}/" >/dev/null; then
    health_ok=1
    break
  fi
  echo "Waiting for LILLY HTTP health checks (${attempt}/30)..."
  sleep 2
done

if (( health_ok != 1 )); then
  compose_new ps >&2 || true
  compose_new logs --tail=100 backend frontend caddy >&2 || true
  rollback
  exit 1
fi

ln -sfn "${RELEASE_DIR}" "${LILLY_ROOT}/current.next"
mv -Tf "${LILLY_ROOT}/current.next" "${LILLY_ROOT}/current"

echo "LILLY release ${RELEASE_ID} is healthy and active."
compose_new ps
