#!/usr/bin/env python3
"""Host-only configuration, snapshot validation, retention and backup alerts."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from email.message import EmailMessage
from email.utils import parseaddr
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import smtplib
import ssl
import subprocess
import sys
import tarfile
import tempfile
import uuid


class OperationsError(ValueError):
    """An operator-readable error containing no configuration values."""


def env_line(key: str, value: str, *, compose: bool = False) -> str:
    return f"{key}={json.dumps(value.replace('$', '$$') if compose else value, ensure_ascii=False)}\n"


def read_env(path: Path, *, compose: bool = False) -> dict[str, str]:
    raw = path.read_bytes()
    try:
        content = raw.decode("utf-8-sig")
    except UnicodeDecodeError:
        content = raw.decode("cp1252")
    values = {}
    for line in content.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, separator, value = line.partition("=")
        key, value = key.strip(), value.strip()
        single_quoted = value.startswith("'")
        if not separator or not re.fullmatch(r"[A-Z][A-Z0-9_]*", key) or key in values:
            raise OperationsError("Invalid or duplicate configuration key")
        if value.startswith('"'):
            try:
                decoded, end = json.JSONDecoder().raw_decode(value)
            except json.JSONDecodeError as error:
                # Preserve compatibility with older dotenv files using shell-style
                # backslash escapes; shlex tokenizes but never executes or expands.
                try:
                    parts = shlex.split(value, comments=True)
                except ValueError:
                    raise OperationsError("Invalid quoted configuration value") from error
                if len(parts) != 1:
                    raise OperationsError("Invalid quoted configuration value") from error
                decoded, end = parts[0], len(value)
            if not isinstance(decoded, str) or (value[end:].strip() and not value[end:].lstrip().startswith("#")):
                raise OperationsError("Invalid quoted configuration value")
            value = decoded
        elif value.startswith("'"):
            try:
                parts = shlex.split(value, comments=True)
            except ValueError as error:
                raise OperationsError("Invalid quoted configuration value") from error
            if len(parts) != 1:
                raise OperationsError("Invalid quoted configuration value")
            value = parts[0]
        else:
            value = value.split(" #", 1)[0].rstrip()
        if any(character in value for character in "\r\n\0"):
            raise OperationsError("Configuration values must be single-line")
        if compose and not single_quoted:
            value = value.replace("$$", "$")
        values[key] = value
    return values


def private_write(path: Path, content: str | bytes) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content.encode("utf-8") if isinstance(content, str) else content)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def email_addresses(value: str) -> tuple[str, ...]:
    addresses = []
    for item in value.split(","):
        item = item.strip()
        if (not item or parseaddr(item)[1] != item
                or not re.fullmatch(r"[^\s@,;<>]+@[^\s@,;<>]+\.[^\s@,;<>]+", item)):
            raise OperationsError("Invalid BACKUP_ALERT_EMAILS")
        if item.lower() not in [address.lower() for address in addresses]:
            addresses.append(item)
    return tuple(addresses)


DEFAULTS = {
    "BACKUP_RETENTION_DAYS": "14",
    "BACKUP_ON_CALENDAR": "*-*-* 03:30:00 Europe/Berlin",
    "BACKUP_RANDOMIZED_DELAY": "15m",
    "BACKUP_ALERT_TIMEOUT_SECONDS": "10",
}


@dataclass(frozen=True)
class Settings:
    root: Path
    backup_dir: Path
    retention: int
    calendar: str
    delay: str
    recipients: tuple[str, ...]
    timeout: int

    @classmethod
    def load(cls, root: Path) -> Settings:
        root = root.resolve(strict=True)
        path = root / "shared/.env.operations"
        if os.name == "posix" and path.stat().st_mode & 0o077:
            raise OperationsError("Operations configuration must have mode 0600")
        values = read_env(path)
        if values.keys() - (DEFAULTS.keys() | {"BACKUP_DIR", "BACKUP_ALERT_EMAILS"}):
            raise OperationsError("Unknown operations configuration key")
        values = {**DEFAULTS, **values}
        backup = Path(values.get("BACKUP_DIR", str(root / "backups")))
        if (not backup.is_absolute() or backup != backup.resolve()
                or backup in (Path("/"), root, root.parent)
                or len(backup.parts) < 3
                or any(backup == root / name or (root / name).is_relative_to(backup)
                       or backup.is_relative_to(root / name)
                       for name in ("shared", "releases", "current", "infrastructure"))):
            raise OperationsError("BACKUP_DIR must be a dedicated absolute directory outside live configuration")
        protected = ("/var/lib/docker", "/var/lib/mysql", "/etc", "/proc", "/sys", "/dev", "/run", "/usr")
        if any(backup == Path(name) or backup.is_relative_to(Path(name)) for name in protected):
            raise OperationsError("BACKUP_DIR overlaps system or live database storage")
        if backup.exists() and not backup.is_dir():
            raise OperationsError("BACKUP_DIR is not a directory")
        try:
            retention = int(values["BACKUP_RETENTION_DAYS"])
            timeout = int(values["BACKUP_ALERT_TIMEOUT_SECONDS"])
        except ValueError as error:
            raise OperationsError("Invalid numeric backup setting") from error
        if not 1 <= retention <= 14 or not 1 <= timeout <= 60:
            raise OperationsError("Backup retention must be 1..14 days and alert timeout 1..60 seconds")
        calendar, delay = values["BACKUP_ON_CALENDAR"], values["BACKUP_RANDOMIZED_DELAY"]
        if not calendar or not re.fullmatch(r"[A-Za-z0-9*/,:. +_-]+", calendar):
            raise OperationsError("Invalid BACKUP_ON_CALENDAR")
        if not re.fullmatch(r"\d+(?:s|min|m|h)?", delay):
            raise OperationsError("Invalid BACKUP_RANDOMIZED_DELAY")
        return cls(root, backup, retention, calendar, delay,
                   email_addresses(values.get("BACKUP_ALERT_EMAILS", "")), timeout)


SNAPSHOT_NAME = re.compile(r"\.?([0-9]{8}T[0-9]{6}Z)-(?:[a-f0-9]{8}-)?(manual|daily|pre-deploy)(\.incomplete)?")


def snapshot_time(name: str) -> datetime | None:
    match = SNAPSHOT_NAME.fullmatch(name)
    if not match:
        return None
    try:
        return datetime.strptime(match[1], "%Y%m%dT%H%M%SZ").replace(tzinfo=timezone.utc)
    except ValueError:
        return None


def rotate(settings: Settings, now: datetime | None = None) -> list[str]:
    cutoff = (now or datetime.now(timezone.utc)) - timedelta(days=settings.retention)
    removed = []
    for directory in settings.backup_dir.iterdir():
        created = snapshot_time(directory.name)
        if created is None or created > cutoff or directory.is_symlink() or not directory.is_dir():
            continue
        if directory.resolve().parent != settings.backup_dir:
            raise OperationsError("Backup rotation escaped its root")
        # Never follow nested symlinks; shutil.rmtree uses descriptor-relative removal on Linux.
        shutil.rmtree(directory)
        removed.append(directory.name)
    with notification_lock(settings.root / "shared/backup-alerts/.lock"):
        prune_alert_status(settings.root, now)
    return removed


def compose_command(root: Path, compose: Path, project: str | None = None) -> list[str]:
    project = project or os.environ.get("LILLY_PROJECT_NAME", "lilly")
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", project):
        raise OperationsError("Invalid Compose project name")
    return ["docker", "compose", "--project-name", project,
            "--env-file", str(root / "shared/.env.production"),
            "--env-file", str(root / "shared/.deployment.env"), "--file", str(compose)]


RECOVERY_SCRIPTS = ("backup.sh", "restore.sh", "operations.py", "configure-environment.py",
                    "configure-operations.py", "notify-backup-failure.py")


def current_recovery_scripts() -> Path:
    # The running helper belongs to the candidate/tool bundle, which can differ
    # from the database-matching infrastructure during a pre-upgrade snapshot.
    return Path(__file__).resolve().parent


def validate_recovery_tools(path: Path) -> None:
    validate_archive(path)
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        if (len(members) != len(RECOVERY_SCRIPTS)
                or {member.name for member in members} != set(RECOVERY_SCRIPTS)
                or not all(member.isfile() for member in members)):
            raise OperationsError("Recovery archive must contain the complete supported tool bundle")


def finish_snapshot(settings: Settings, directory: Path, compose: Path) -> None:
    created = snapshot_time(directory.name)
    if created is None or directory.is_symlink() or directory.resolve().parent != settings.backup_dir:
        raise OperationsError("Snapshot is outside BACKUP_DIR")
    configuration = directory / "configuration"
    configuration.mkdir(mode=0o700)
    for source, destination in ((".env.production", "production.env"),
                                (".deployment.env", "deployment.env"),
                                (".env.operations", "operations.env")):
        private_write(configuration / destination, (settings.root / "shared" / source).read_bytes())
    infrastructure = compose.resolve().parent
    with tarfile.open(directory / "deployment.tar.gz", "w:gz") as archive:
        for name in ("docker-compose.production.yml", "Caddyfile", "systemd"):
            source = infrastructure / name
            if source.exists():
                archive.add(source, arcname=name, filter=lambda member: member if "__pycache__" not in member.name else None)
    tools = current_recovery_scripts()
    with tarfile.open(directory / "recovery-tools.tar.gz", "w:gz") as archive:
        for name in RECOVERY_SCRIPTS:
            source = tools / name
            if source.is_symlink() or not source.is_file():
                raise OperationsError("Current recovery tool bundle is incomplete")
            archive.add(source, arcname=name)
    result = subprocess.run([*compose_command(settings.root, compose), "config", "--images", "db"],
                            check=True, capture_output=True, text=True)
    image = result.stdout.strip()
    if not image or "\n" in image:
        raise OperationsError("Cannot determine the database image")
    manifest = {"format": 3, "created_utc": created.isoformat(),
                "database_image": image, "infrastructure_release": infrastructure.name,
                "recovery_tools_sha256": file_hash(directory / "recovery-tools.tar.gz")}
    private_write(directory / "manifest.json", json.dumps(manifest, sort_keys=True) + "\n")
    names = ["database.sql.gz", "media.tar.gz", "account-erasure.log", "metadata.env",
             "deployment.tar.gz", "recovery-tools.tar.gz", "manifest.json", "configuration/production.env",
             "configuration/deployment.env", "configuration/operations.env"]
    checksums = "".join(f"{file_hash(directory / name)}  {name}\n" for name in names)
    private_write(directory / "SHA256SUMS", checksums)


def file_hash(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate_archive(path: Path) -> None:
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            relative = PurePosixPath(member.name)
            if relative.is_absolute() or ".." in relative.parts or not (member.isfile() or member.isdir()):
                raise OperationsError("Unsafe member in backup archive")


def verify_snapshot(directory: Path) -> dict:
    if directory.is_symlink() or not (directory / "COMPLETE").is_file():
        raise OperationsError("Snapshot is incomplete")
    required = {"database.sql.gz", "media.tar.gz", "account-erasure.log"}
    versioned_files = required | {"metadata.env", "deployment.tar.gz", "manifest.json",
                          "configuration/production.env", "configuration/deployment.env", "configuration/operations.env"}
    allowed = versioned_files | {"recovery-tools.tar.gz"}
    checked = set()
    for line in (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        digest, separator, name = line.partition("  ")
        path = directory / name
        if (not separator or not re.fullmatch(r"[a-f0-9]{64}", digest)
                or name not in allowed or name in checked or path.is_symlink()
                or path.resolve().parent not in (directory.resolve(), (directory / "configuration").resolve())
                or path.resolve() != path.absolute() or not path.is_file()):
            raise OperationsError("Invalid snapshot checksum entry")
        if file_hash(path) != digest:
            raise OperationsError("Snapshot checksum mismatch")
        checked.add(name)
    if not required <= checked:
        raise OperationsError("Snapshot checksums omit required files")
    manifest = {}
    if (directory / "manifest.json").exists():
        manifest = json.loads((directory / "manifest.json").read_text())
        if (not isinstance(manifest, dict) or manifest.get("format") not in (2, 3)
                or not isinstance(manifest.get("infrastructure_release"), str)
                or not re.fullmatch(r"[a-f0-9]{40}", manifest["infrastructure_release"])
                or not isinstance(manifest.get("database_image"), str)
                or not re.fullmatch(r"[A-Za-z0-9./:_@-]{1,512}", manifest["database_image"])):
            raise OperationsError("Invalid snapshot manifest")
        expected = allowed if manifest["format"] == 3 else versioned_files
        if checked != expected:
            raise OperationsError("Versioned snapshot is missing recovery files")
        if manifest["format"] == 3:
            if manifest.get("recovery_tools_sha256") != file_hash(directory / "recovery-tools.tar.gz"):
                raise OperationsError("Recovery tool bundle does not match its manifest")
            validate_recovery_tools(directory / "recovery-tools.tar.gz")
        validate_archive(directory / "deployment.tar.gz")
    with gzip.open(directory / "database.sql.gz", "rb") as stream:
        while stream.read(1024 * 1024):
            pass
    validate_archive(directory / "media.tar.gz")
    return manifest


def check_database_restore(source: str, target: str) -> None:
    """Permit exact images and tested forward logical imports, never a downgrade."""
    if not source or source == target:
        return
    pattern = r"(?:docker\.io/(?:library/)?)?mariadb:([0-9]+)\.([0-9]+)\.([0-9]+)@sha256:[a-f0-9]{64}"
    original, running = re.fullmatch(pattern, source), re.fullmatch(pattern, target)
    if original and running:
        before, after = tuple(map(int, original.groups())), tuple(map(int, running.groups()))
        if before <= after and (before[:2] == after[:2] or (before[:2], after[:2]) == ((12, 3), (13, 0))):
            return
    raise OperationsError("Unsupported logical database restore; use an isolated rebuild with the recorded image")


def recover_configuration(directory: Path, root: Path, live_ledger: Path, owner: str | None = None) -> None:
    """Prepare an empty installation; never replace a live configuration or ledger."""
    manifest = verify_snapshot(directory)
    if not manifest:
        raise OperationsError("Fresh-host recovery requires a version 2 or 3 snapshot")
    if live_ledger.resolve() == (directory / "account-erasure.log").resolve():
        raise OperationsError("Provide the latest separately retained live ledger")
    ledger = live_ledger.read_bytes()
    if any(not re.fullmatch(rb"[a-fA-F0-9]{64}", line.strip()) for line in ledger.splitlines() if line.strip()):
        raise OperationsError("Invalid live erasure ledger")
    root = root.resolve()
    shared = root / "shared"
    if any((root / name).is_symlink() for name in ("shared", "releases", "infrastructure")):
        raise OperationsError("Recovery requires fresh configuration and release directories")
    targets = [shared / name for name in (".env.production", ".deployment.env", ".env.operations")]
    entrypoints = [shared / name for name in ("backup.sh", "backup-tools", "recovery-tools")]
    if any(path.exists() or path.is_symlink() for path in [*entrypoints, root / "infrastructure", root / "current"]):
        raise OperationsError("Recovery refuses to overwrite existing tools or release references")
    target_ledger = shared / "erasure-ledger/account-erasure.log"
    if any(path.exists() or path.is_symlink() for path in targets) or target_ledger.is_symlink() or target_ledger.parent.is_symlink():
        raise OperationsError("Recovery refuses to overwrite an existing installation")
    if target_ledger.exists() and target_ledger.stat().st_size:
        raise OperationsError("Recovery refuses to replace a nonempty live ledger")
    release = root / "releases" / manifest["infrastructure_release"]
    if release.exists() or release.is_symlink():
        raise OperationsError("Recovery release already exists")
    uid_gid = None
    if owner is not None:
        import pwd
        account = pwd.getpwnam(owner)
        uid_gid = account.pw_uid, account.pw_gid
    release.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix=".recover-", dir=release.parent))
    temporary_tools = Path(tempfile.mkdtemp(prefix=".recover-tools-", dir=release.parent))
    try:
        with tarfile.open(directory / "deployment.tar.gz", "r:gz") as archive:
            archive.extractall(temporary, filter="data")
        if manifest["format"] == 3:
            with tarfile.open(directory / "recovery-tools.tar.gz", "r:gz") as archive:
                archive.extractall(temporary_tools, filter="data")
            tools_id = manifest["recovery_tools_sha256"]
        else:
            # Old snapshots did not separate recovery code. Use this current,
            # explicitly invoked helper bundle rather than archived legacy scripts.
            for name in RECOVERY_SCRIPTS:
                shutil.copyfile(current_recovery_scripts() / name, temporary_tools / name)
            tools_id = hashlib.sha256(b"".join(name.encode() + (temporary_tools / name).read_bytes()
                                              for name in RECOVERY_SCRIPTS)).hexdigest()
        for path in temporary_tools.iterdir():
            os.chmod(path, 0o700 if path.suffix == ".sh" else 0o600)
        tools_parent = shared / "recovery-tools"
        tools_parent.mkdir(mode=0o750, parents=True)
        tools = tools_parent / tools_id
        os.replace(temporary_tools, tools)
        os.chmod(tools, 0o750)
        os.replace(temporary, release)
        os.chmod(release, 0o750)
        if uid_gid:
            for path in (root, shared, release.parent, release, tools_parent, tools):
                path.mkdir(mode=0o750, parents=True, exist_ok=True)
                os.chown(path, *uid_gid)
            for path in release.rglob("*"):
                os.chown(path, *uid_gid)
            for path in tools.iterdir():
                os.chown(path, *uid_gid)
        for source, target in zip(("production.env", "deployment.env", "operations.env"), targets, strict=True):
            content = (directory / "configuration" / source).read_bytes()
            if source == "operations.env":
                values = read_env(directory / "configuration" / source)
                values["BACKUP_DIR"] = str(root / "backups")
                content = "".join(f"{key}={json.dumps(value)}\n" for key, value in values.items()).encode()
            private_write(target, content)
            if uid_gid:
                os.chown(target, *uid_gid)
        private_write(target_ledger, ledger)
        if os.name == "posix" and os.geteuid() == 0:
            os.chmod(target_ledger.parent, 0o770)
            os.chown(target_ledger.parent, uid_gid[0] if uid_gid else os.geteuid(), 999)
            os.chown(target_ledger, 999, 999)
        (root / "infrastructure").symlink_to(release, target_is_directory=True)
        private_write(shared / "backup.sh", (tools / "backup.sh").read_bytes())
        os.chmod(shared / "backup.sh", 0o700)
        (shared / "backup-tools").symlink_to(tools, target_is_directory=True)
        if uid_gid:
            os.chown(shared / "backup.sh", *uid_gid)
            os.chown(shared / "backup-tools", *uid_gid, follow_symlinks=False)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
        if temporary_tools.exists():
            shutil.rmtree(temporary_tools)


@contextmanager
def notification_lock(path: Path):
    import fcntl

    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with path.open("a") as stream:
        os.chmod(path, 0o600)
        fcntl.flock(stream, fcntl.LOCK_EX)
        yield


def send_alert(mail: dict[str, str], recipient: str, message: EmailMessage, timeout: int) -> None:
    mode = mail.get("SMTP_TLS_MODE", "starttls")
    host = mail.get("SMTP_HOST", "")
    if not host or mode not in {"starttls", "tls", "none"}:
        raise OperationsError("SMTP_HOST and a valid SMTP_TLS_MODE are required")
    if mode == "none" and os.environ.get("LILLY_ISOLATED_TEST") != "1":
        raise OperationsError("Unencrypted SMTP is allowed only in isolated tests")
    client = None
    try:
        arguments = dict(host=host, port=int(mail.get("SMTP_PORT", "587")), timeout=timeout)
        client = smtplib.SMTP_SSL(**arguments, context=ssl.create_default_context()) if mode == "tls" else smtplib.SMTP(**arguments)
        if mode == "starttls":
            client.starttls(context=ssl.create_default_context())
        user, password = mail.get("SMTP_USER", ""), mail.get("SMTP_PASSWORD", "")
        if bool(user) != bool(password):
            raise OperationsError("SMTP_USER and SMTP_PASSWORD must be configured together")
        if user:
            client.login(user, password)
        refused = client.send_message(message, from_addr=mail["SMTP_FROM"], to_addrs=[recipient])
        if refused:
            raise OperationsError("SMTP recipient was refused")
    finally:
        if client is not None:
            client.close()


def prune_alert_status(root: Path, now: datetime | None = None) -> None:
    """Expire delivery receipts while the caller holds the notification lock."""
    cutoff = (now or datetime.now(timezone.utc)).timestamp() - 14 * 86400
    for old in (root / "shared/backup-alerts").glob("*.json"):
        if not old.is_symlink() and old.stat().st_mtime < cutoff:
            old.unlink()


def notify_failure(root: Path, run_id: str, kind: str, phase: str) -> bool:
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,80}", run_id):
        raise OperationsError("Invalid backup run identifier")
    root = root.resolve(strict=True)
    # A bad retention/calendar setting must not prevent reporting that configuration error.
    values = read_env(root / "shared/.env.operations")
    recipients = email_addresses(values.get("BACKUP_ALERT_EMAILS", ""))
    timeout = int(values.get("BACKUP_ALERT_TIMEOUT_SECONDS", "10"))
    if not 1 <= timeout <= 60:
        raise OperationsError("Invalid alert timeout")
    mail = read_env(root / "shared/.env.production", compose=True)
    email_addresses(mail.get("SMTP_FROM", ""))
    status_dir = root / "shared/backup-alerts"
    with notification_lock(status_dir / ".lock"):
        status_path = status_dir / (run_id + ".json")
        delivered = set(json.loads(status_path.read_text()) if status_path.exists() else [])
        safe_kind = kind if kind in {"manual", "daily", "pre-deploy", "service"} else "unknown"
        safe_phase = phase if phase in {"configuration", "lock", "database", "media", "ledger", "resume", "snapshot", "rotation", "service"} else "unknown"
        for recipient in recipients:
            if recipient.lower() in delivered:
                continue
            message = EmailMessage()
            message["From"], message["To"] = mail["SMTP_FROM"], recipient
            message["Subject"] = "LILLY backup failed"
            message.set_content(f"UTC: {datetime.now(timezone.utc).isoformat()}\nRun: {run_id}\nKind: {safe_kind}\nPhase: {safe_phase}\nInspect journalctl -u lilly-backup.service on the LILLY host.\n")
            for attempt in range(2):
                try:
                    send_alert(mail, recipient, message, timeout)
                except (OSError, ValueError, smtplib.SMTPException):
                    print(f"Backup failure notification attempt {attempt + 1} failed", file=sys.stderr)
                else:
                    delivered.add(recipient.lower())
                    private_write(status_path, json.dumps(sorted(delivered)))
                    break
        prune_alert_status(root)
        return all(recipient.lower() in delivered for recipient in recipients)


def configure(settings: Settings, systemd_directory: Path, apply: bool) -> None:
    subprocess.run(["systemd-analyze", "calendar", settings.calendar], check=True, capture_output=True)
    settings.backup_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(settings.backup_dir, 0o700)
    if os.name == "posix" and os.geteuid() == 0:
        owner = (settings.root / "shared/.env.operations").stat()
        os.chown(settings.backup_dir, owner.st_uid, owner.st_gid)
    drop_in = systemd_directory / "lilly-backup.timer.d/override.conf"
    private_write(drop_in, f"[Timer]\nOnCalendar=\nOnCalendar={settings.calendar}\nRandomizedDelaySec={settings.delay}\n")
    if apply:
        subprocess.run(["systemctl", "daemon-reload"], check=True)
        subprocess.run(["systemctl", "show", "lilly-backup.timer", "--property=TimersCalendar", "--property=RandomizedDelayUSec"], check=True)


def main(command: str | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(os.environ.get("LILLY_ROOT", "/opt/lilly")))
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("settings")
    sub.add_parser("rotate")
    finish = sub.add_parser("finish")
    finish.add_argument("directory", type=Path)
    finish.add_argument("compose", type=Path)
    verify = sub.add_parser("verify")
    verify.add_argument("directory", type=Path)
    verify.add_argument("--database-image", action="store_true")
    database = sub.add_parser("check-database-restore")
    database.add_argument("--source-image", required=True)
    database.add_argument("--target-image", required=True)
    notice = sub.add_parser("notify")
    notice.add_argument("--run-id", default=os.environ.get("MONITOR_INVOCATION_ID") or os.environ.get("INVOCATION_ID") or uuid.uuid4().hex)
    notice.add_argument("--kind", default="service")
    notice.add_argument("--phase", default="service")
    configuration = sub.add_parser("configure")
    configuration.add_argument("--systemd-directory", type=Path, default=Path("/etc/systemd/system"))
    configuration.add_argument("--apply", action="store_true")
    recovery = sub.add_parser("recover-files")
    recovery.add_argument("directory", type=Path)
    recovery.add_argument("--live-ledger", type=Path, required=True)
    recovery.add_argument("--owner")
    arguments = sys.argv[1:]
    if command:
        # Wrapper scripts accept --root before or after their implicit command.
        root_arguments = []
        if "--root" in arguments:
            index = arguments.index("--root")
            root_arguments = arguments[index:index + 2]
            arguments = arguments[:index] + arguments[index + 2:]
        arguments = [*root_arguments, command, *arguments]
    args = parser.parse_args(arguments)
    try:
        if args.command == "verify":
            manifest = verify_snapshot(args.directory)
            if args.database_image:
                print(manifest.get("database_image", ""))
        elif args.command == "check-database-restore":
            check_database_restore(args.source_image, args.target_image)
        elif args.command == "recover-files":
            recover_configuration(args.directory, args.root, args.live_ledger, args.owner)
        elif args.command == "notify":
            return 0 if notify_failure(args.root, args.run_id, args.kind, args.phase) else 1
        else:
            settings = Settings.load(args.root)
            if args.command == "settings":
                print(settings.backup_dir)
                print(settings.retention)
            elif args.command == "rotate":
                rotate(settings)
            elif args.command == "finish":
                finish_snapshot(settings, args.directory, args.compose)
            elif args.command == "configure":
                configure(settings, args.systemd_directory, args.apply)
    except (OSError, ValueError, subprocess.SubprocessError, tarfile.TarError) as error:
        detail = str(error) if isinstance(error, OperationsError) else type(error).__name__
        print(f"Operations {args.command} failed: {detail}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
