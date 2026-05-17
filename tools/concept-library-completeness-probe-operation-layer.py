#!/usr/bin/env python3
"""Generate the manifest-driven operation-layer completeness probe."""

from __future__ import annotations

from concept_library_completeness_probe_lib import run_operation_probe


def main() -> int:
    path = run_operation_probe()
    print(f"Written: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
