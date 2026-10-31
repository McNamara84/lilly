#!/usr/bin/env python3
"""Export or check the API contract without starting a server or opening a database."""

import argparse
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if the checked-in contract is stale")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ["cargo", "run", "--locked", "--quiet", "-p", "lilly-backend", "--", "openapi", "export"],
        cwd=root,
        env={**os.environ, "SQLX_OFFLINE": "true"},
        stdout=subprocess.PIPE,
        check=True,
    )
    destination = root / "docs/openapi.json"
    # Read text to tolerate Git's CRLF checkout on Windows; always export UTF-8/LF.
    generated = result.stdout.decode("utf-8").replace("\r\n", "\n")
    if args.check:
        if not destination.exists() or destination.read_text(encoding="utf-8") != generated:
            print("OpenAPI contract is stale. Run: python scripts/openapi.py", file=sys.stderr)
            return 1
        print("OpenAPI contract is current.")
    else:
        destination.write_text(generated, encoding="utf-8", newline="\n")
        print("Exported docs/openapi.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
