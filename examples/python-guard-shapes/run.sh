#!/usr/bin/env bash
# Guard-shapes matrix: the four runtime-guard bug classes across numpy AND
# pandas, proved by the witness axis (it RUNS the real library code).
#
#   #2 index-bounds    #3 empty-container    #4 divide-by-zero    #5 key-access
#
# Each cell is a pair: a guarded `_ok` case the witness DISCHARGES, and a `_bad`
# case that breaches the guard (IndexError / ValueError / KeyError / silent inf)
# so the witness REFUSES it. 8 cells x 2 = 16 files; the witness is per-file, so
# ok and bad live in separate files.
#
# This script PASSES iff provekit discharges every `_ok` and refuses every
# `_bad` -- checked per file, so a swapped verdict fails the gate.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BIN="$REPO/implementations/rust/target/debug/provekit"

VENV="${PANDAS_WITNESS_VENV:-/tmp/pandas-witness-venv}"
if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q numpy pandas pytest pynacl blake3 cbor2
fi

cd "$HERE"
rm -f blake3-512:*.proof 2>/dev/null || true
rm -rf .provekit/runs .provekit/witnesses __pycache__ 2>/dev/null || true

echo "== mint (pytest-witness over the 16 cases) =="
"$BIN" mint --out . --quiet

echo "== prove (witness axis: re-run each case under real numpy/pandas) =="
PATH="$VENV/bin:$PATH" "$BIN" prove . --json 2>/dev/null > /tmp/guard_shapes_prove.json

echo "== self-check: every _ok discharged, every _bad refused (per file) =="
"$VENV/bin/python" - <<'PY'
import json, sys
d = json.load(open("/tmp/guard_shapes_prove.json"))
rows = d.get("rows", [])
seen = {}
for r in rows:
    prop = r.get("property", "")
    f = prop.split(":")[-1]          # "...:test_<shape>_<lib>_<kind>.py"
    if f.startswith("test_") and f.endswith(".py"):
        seen[f] = r.get("status")
fail = 0
shapes = ["index_bounds", "empty_container", "divide_by_zero", "key_access"]
for shape in shapes:
    for lib in ("numpy", "pandas"):
        ok, bad = f"test_{shape}_{lib}_ok.py", f"test_{shape}_{lib}_bad.py"
        so, sb = seen.get(ok), seen.get(bad)
        ok_pass  = so == "discharged"
        bad_pass = sb == "unsatisfied"
        mark = "ok " if (ok_pass and bad_pass) else "XX "
        if not (ok_pass and bad_pass): fail = 1
        print(f"  {mark}{shape:<16} {lib:<6}  ok->{so}  bad->{sb}")
disc = sum(1 for v in seen.values() if v == "discharged")
refu = sum(1 for v in seen.values() if v == "unsatisfied")
print(f"\n  totals: {disc} discharged, {refu} refused (expected 8 / 8)")
if fail or disc != 8 or refu != 8:
    print("\nFAIL: provekit did not produce the expected per-cell verdict."); sys.exit(1)
print("\nPASS: 2x4 guard-shape matrix -- every guarded case proved, every violation refused.")
PY