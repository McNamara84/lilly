#!/usr/bin/env python3
"""Validate host backup settings and install the systemd timer override."""

import sys
from operations import main

if __name__ == "__main__":
    sys.exit(main("configure"))
