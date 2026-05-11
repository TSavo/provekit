#!/bin/sh
set -eu
BASE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest "$BASE/scripts/test_discharge.py"
PYTHONDONTWRITEBYTECODE=1 python3 "$BASE/scripts/discharge.py"
PYTHONDONTWRITEBYTECODE=1 python3 "$BASE/scripts/primitive_ops.py"
