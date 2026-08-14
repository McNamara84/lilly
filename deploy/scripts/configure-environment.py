#!/usr/bin/env python3
"""Create LILLY's server-only environment without printing secret values."""

from __future__ import annotations

import argparse
import json
import os
import pwd
import secrets
import shlex
import tempfile
from pathlib import Path
from urllib.parse import quote


def parse_dotenv(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    raw_bytes = path.read_bytes()
    try:
        text = raw_bytes.decode("utf-8")
    except UnicodeDecodeError:
        # Older server-side environment files may have been saved by a
        # Windows editor. CP1252 covers the encoding used by those editors
        # without exposing or silently dropping secret characters.
        text = raw_bytes.decode("cp1252")
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, raw_value = line.split("=", 1)
        key = key.strip()
        raw_value = raw_value.strip()
        if raw_value.startswith(("'", '"')):
            try:
                parts = shlex.split(raw_value, comments=True, posix=True)
            except ValueError as error:
                raise ValueError(f"Invalid quoted value for {key}") from error
            value = parts[0] if parts else ""
        else:
            value = raw_value.split(" #", 1)[0].rstrip()
        values[key] = value
    return values


def require(values: dict[str, str], key: str) -> str:
    value = values.get(key, "").strip()
    if not value:
        raise ValueError(f"Required source mail setting is missing: {key}")
    return value


def dotenv_line(key: str, value: str) -> str:
    return f"{key}={json.dumps(value, ensure_ascii=False)}\n"


def write_private_file(path: Path, content: str, owner: str) -> None:
    if path.exists():
        raise FileExistsError(f"Refusing to overwrite existing environment: {path}")
    path.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
    account = pwd.getpwnam(owner)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.", dir=path.parent, text=True
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary_path, 0o600)
        os.chown(temporary_path, account.pw_uid, account.pw_gid)
        temporary_path.replace(path)
    finally:
        temporary_path.unlink(missing_ok=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mail-source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--owner", default="lilly-deploy")
    parser.add_argument("--app-base-url", required=True)
    parser.add_argument("--admin-email", default="")
    args = parser.parse_args()

    source = parse_dotenv(args.mail_source)
    encryption = source.get("MAIL_ENCRYPTION", "").split()[0].lower()
    tls_mode = "tls" if encryption in {"ssl", "smtps"} else "starttls"

    database_password = secrets.token_hex(32)
    database_root_password = secrets.token_hex(32)
    jwt_secret = secrets.token_urlsafe(64)
    database_url = (
        "mysql://lilly:"
        f"{quote(database_password, safe='')}"
        "@db:3306/lilly"
    )

    settings = {
        "MARIADB_ROOT_PASSWORD": database_root_password,
        "MARIADB_DATABASE": "lilly",
        "MARIADB_USER": "lilly",
        "MARIADB_PASSWORD": database_password,
        "DATABASE_URL": database_url,
        "JWT_SECRET": jwt_secret,
        "JWT_ACCESS_TOKEN_EXPIRY": "900",
        "JWT_REFRESH_TOKEN_EXPIRY": "2592000",
        "PASSWORD_RESET_TTL_SECONDS": "3600",
        "GOOGLE_OAUTH_CLIENT_ID": "",
        "GOOGLE_OAUTH_CLIENT_SECRET": "",
        "GITHUB_OAUTH_CLIENT_ID": "",
        "GITHUB_OAUTH_CLIENT_SECRET": "",
        "PRIVACY_POLICY_VERSION": "2026-08-14",
        "ADMIN_EMAIL": args.admin_email.strip(),
        "APP_BASE_URL": args.app_base_url,
        "COOKIE_SECURE": "false",
        "TRUSTED_PROXY_CIDRS": "172.16.0.0/12",
        "RATE_LIMIT_REGISTER": "5/900",
        "RATE_LIMIT_LOGIN_CLIENT": "30/900",
        "RATE_LIMIT_LOGIN_ACCOUNT": "10/900",
        "RATE_LIMIT_RESEND_VERIFICATION": "5/900",
        "RATE_LIMIT_PASSWORD_RESET_REQUEST": "5/900",
        "RATE_LIMIT_PASSWORD_RESET_CONFIRM": "10/900",
        "RATE_LIMIT_OAUTH_START": "10/60",
        "RATE_LIMIT_OAUTH_CALLBACK": "30/300",
        "RATE_LIMIT_REFRESH": "60/60",
        "RATE_LIMIT_PUBLIC_API": "120/60",
        "RATE_LIMIT_AUTHENTICATED_API": "600/60",
        "SMTP_HOST": require(source, "MAIL_HOST"),
        "SMTP_PORT": require(source, "MAIL_PORT"),
        "SMTP_TLS_MODE": tls_mode,
        "SMTP_USER": require(source, "MAIL_USERNAME"),
        "SMTP_PASSWORD": require(source, "MAIL_PASSWORD"),
        "SMTP_FROM": require(source, "MAIL_FROM_ADDRESS"),
        "RUST_LOG": "info",
        "PHOTO_MAX_UPLOAD_BYTES": "5242880",
        "PHOTO_MAX_EDGE": "2048",
        "PHOTO_MAX_SOURCE_DIMENSION": "10000",
        "PHOTO_MAX_SOURCE_PIXELS": "40000000",
        "PHOTO_JPEG_QUALITY": "85",
        "IMPORT_SCHEDULER_ENABLED": "false",
        "IMPORT_SCHEDULE": "0 10 6 * * Sat *",
        "IMPORT_TIMEZONE": "Europe/Berlin",
        "IMPORT_SCHEDULED_ADAPTERS": "maddrax,john-sinclair",
    }
    content = "".join(dotenv_line(key, value) for key, value in settings.items())
    write_private_file(args.output, content, args.owner)
    print(f"Created private LILLY environment at {args.output}")


if __name__ == "__main__":
    main()
