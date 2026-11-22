#!/usr/bin/env python3
"""Notify operations addresses after a failed systemd backup invocation."""

import sys
from operations import main

if __name__ == "__main__":
    sys.exit(main("notify"))
