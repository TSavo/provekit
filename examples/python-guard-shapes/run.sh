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
# This script PASSES iff sugar discharges every `_ok` and refuses every
# `_bad` -- checked per file, so a swapped verdict fails the gate.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BIN="$REPO/implementations/rust/target/debug/sugar"

VENV="${PANDAS_WITNESS_VENV:-/tmp/pandas-witness-venv}"
if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q numpy pandas pytest pynacl blake3 cbor2
fi

cd "$HERE"
rm -f blake3-512:*.proof 2>/dev/null || true
rm -rf .sugar/runs .sugar/witnesses __pycache__ 2>/dev/null || true

echo "== mint (one WitnessPackageMemento over the 16 cases; oracle runs the suite) =="
rm -rf .sugar/witnesses 2>/dev/null || true
"$BIN" mint --out . --quiet

echo "== self-check: the per-test facts live in the content-addressed .witness package =="
# The proof carries ONE package cid; the 16 per-test outcomes are IN the package
# (each line a witness body). We read the package and assert every _ok passed and
# every _bad failed -- per-test discrimination, now pinned by one cid.
"$VENV/bin/python" - <<'PY'
import json, sys, glob
bundles = glob.glob(".sugar/witnesses/*.witness")
if not bundles:
    print("FAIL: no witness package written"); sys.exit(1)
seen = {}
for line in open(bundles[0], "rb"):
    line = line.strip()
    if not line: continue
    w = json.loads(line)
    f = w["test"].split("::")[0]            # node id -> file
    seen[f] = w["outcome"]
fail = 0
for shape in ["index_bounds", "empty_container", "divide_by_zero", "key_access"]:
    for lib in ("numpy", "pandas"):
        ok, bad = f"test_{shape}_{lib}_ok.py", f"test_{shape}_{lib}_bad.py"
        so, sb = seen.get(ok), seen.get(bad)
        good = (so == "passed" and sb != "passed")
        if not good: fail = 1
        print(f"  {'ok ' if good else 'XX '}{shape:<16} {lib:<6}  ok->{so}  bad->{sb}")
passed = sum(1 for v in seen.values() if v == "passed")
failed = sum(1 for v in seen.values() if v != "passed")
print(f"\n  package: {passed} passed, {failed} failed (expected 8 / 8)")
if fail or passed != 8 or failed != 8:
    print("\nFAIL: package did not record the expected per-cell outcomes."); sys.exit(1)
print("\nPASS: 2x4 guard-shape matrix -- one package cid, every guarded case passed, every violation failed.")
PY

echo "== prove: the verifier asks the oracle to reproduce the package (one cid) =="
PATH="$VENV/bin:$PATH" "$BIN" prove . 2>/dev/null | grep -iE 'reproduced|failed:|package' | head -3 || true