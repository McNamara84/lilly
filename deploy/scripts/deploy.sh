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

if [[ ! "${RELEASE_ID}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Usage: $0 <40-character commit SHA>" >&2
  exit 2
fi

for required_file in "${APP_ENV_FILE}" "${COMPOSE_FILE}" "${RELEASE_DIR}/Caddyfile"; do
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

bind_address="0.0.0.0"
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

if [[ "${bind_address}" != "0.0.0.0" && "${bind_address}" != "127.0.0.1" ]]; then
  echo "LILLY_BIND_ADDRESS must be either 0.0.0.0 or 127.0.0.1" >&2
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
    --project-name lilly \
    --env-file "${APP_ENV_FILE}" \
    --env-file "${CANDIDATE_ENV_FILE}" \
    --file "${COMPOSE_FILE}" \
    "$@"
}

rollback() {
  echo "Deployment failed; attempting container rollback." >&2

  if (( had_previous_env == 1 )); then
    install -m 600 "${previous_env_file}" "${DEPLOYMENT_ENV_FILE}"
  else
    rm -f -- "${DEPLOYMENT_ENV_FILE}"
  fi

  if [[ -n "${previous_release}" && -f "${previous_release}/docker-compose.production.yml" && -f "${DEPLOYMENT_ENV_FILE}" ]]; then
    if ! docker compose \
      --project-name lilly \
      --env-file "${APP_ENV_FILE}" \
      --env-file "${DEPLOYMENT_ENV_FILE}" \
      --file "${previous_release}/docker-compose.production.yml" \
      up -d --wait --remove-orphans; then
      echo "Automatic container rollback also failed. Use the pre-deployment backup for manual recovery." >&2
    fi
  else
    compose_new stop || true
  fi
}

echo "Validating release ${RELEASE_ID}..."
compose_new config --quiet

if [[ -n "${previous_release}" && -x "${previous_release}/scripts/backup.sh" ]]; then
  "${previous_release}/scripts/backup.sh" pre-deploy
else
  echo "No active LILLY release exists yet; skipping the initial pre-deployment backup."
fi

echo "Pulling immutable release images..."
compose_new pull

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
