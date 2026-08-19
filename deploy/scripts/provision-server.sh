#!/usr/bin/env bash
set -Eeuo pipefail

readonly DEPLOY_USER="lilly-deploy"
readonly BACKEND_CONTAINER_GID="999"
readonly LILLY_ROOT="/opt/lilly"
readonly PUBLIC_KEY_FILE="${1:-}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
DEPLOY_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
readonly DEPLOY_DIR

if (( EUID != 0 )); then
  echo "This provisioning script must run as root." >&2
  exit 1
fi

if [[ -z "${PUBLIC_KEY_FILE}" || ! -f "${PUBLIC_KEY_FILE}" ]]; then
  echo "Usage: $0 /path/to/lilly-deploy.pub" >&2
  exit 2
fi

if ! id "${DEPLOY_USER}" >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash "${DEPLOY_USER}"
fi
usermod --append --groups docker "${DEPLOY_USER}"
passwd --lock "${DEPLOY_USER}" >/dev/null

install -d -m 0750 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" \
  "${LILLY_ROOT}" \
  "${LILLY_ROOT}/incoming" \
  "${LILLY_ROOT}/releases" \
  "${LILLY_ROOT}/shared" \
  "${LILLY_ROOT}/backups"
install -d -m 0770 -o "${DEPLOY_USER}" -g "${BACKEND_CONTAINER_GID}" \
  "${LILLY_ROOT}/shared/erasure-ledger"
install -d -m 0700 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" \
  "/home/${DEPLOY_USER}/.ssh"

{
  printf 'no-agent-forwarding,no-port-forwarding,no-X11-forwarding,no-pty '
  tr -d '\r\n' <"${PUBLIC_KEY_FILE}"
  printf '\n'
} >"/home/${DEPLOY_USER}/.ssh/authorized_keys"
chown "${DEPLOY_USER}:${DEPLOY_USER}" "/home/${DEPLOY_USER}/.ssh/authorized_keys"
chmod 0600 "/home/${DEPLOY_USER}/.ssh/authorized_keys"

install -m 0644 "${DEPLOY_DIR}/systemd/lilly-backup.service" \
  /etc/systemd/system/lilly-backup.service
install -m 0644 "${DEPLOY_DIR}/systemd/lilly-backup.timer" \
  /etc/systemd/system/lilly-backup.timer
systemctl daemon-reload

echo "Provisioned ${DEPLOY_USER} and ${LILLY_ROOT}."
echo "Enable lilly-backup.timer only after the first successful release is active."
