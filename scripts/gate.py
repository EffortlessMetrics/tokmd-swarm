#!/usr/bin/env python3
"""Run the complete local gate with low-space Cargo defaults from bootstrap."""

import os
import subprocess
import sys


def main() -> int:
    env = os.environ.copy()
    # Cargo's test profile inherits dev, unless the caller overrides it.
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    env.setdefault("CARGO_INCREMENTAL", "0")
    return subprocess.run(
        ["cargo", "--locked", "xtask", "gate", "--check"], env=env, check=False
    ).returncode


if __name__ == "__main__":
    sys.exit(main())
