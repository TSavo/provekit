#!/usr/bin/env bash
# pandas showcase: the same two-axis correctness claim as numpy-showcase, one
# rung up the ladder. The next library after numpy is NOT a new package -- it is
# the SAME one lifter (provekit_lift_py_tests.assertion_lsp), which learns pandas's
# vocabulary from each test file's imports plus a dropped-in data file,
# .provekit/vocab-exceptions/pandas.testing.json. This example differs from
# numpy-showcase only by that exception file and pointing the witness venv at pandas.
#
#   mint   — three lift surfaces run over the project: the plain pytest CONSISTENCY
#            surface (scalar assertions), the one assertion lifter learning
#            pandas.testing (frame assertions, approximate-by-default REFUSED unless
#            check_exact pinned), and the pytest-witness surface (RUNS the tests
#            under real pandas).
#   prove  — discharges two ways:
#              CONSISTENT : z3 finds the good contracts mutually consistent and
#                           the contradictory one UNSAT.
#              WITNESSED  : the witness re-runs pytest; the good tests reproduce,
#                           the contradictory one's run is 'failed'.
#
# The project deliberately contains a buggy (self-contradictory) test,
# test_pandas_sum_bad.py, so the showcase proves the CORRECT pandas code AND
# catches the contradiction -- refused BOTH ways. This script PASSES iff
# provekit produces exactly that verdict.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BIN="$REPO/implementations/rust/target/debug/provekit"

# The witness lifter RUNS pandas's tests, so it needs pandas + the kit deps in a
# venv (PEP 668: never --break-system-packages). The lift manifests point their
# interpreter at this venv.
VENV="${PANDAS_WITNESS_VENV:-/tmp/pandas-witness-venv}"
if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q pandas pytest pynacl blake3 cbor2
fi

cd "$HERE"
rm -f blake3-512:*.proof 2>/dev/null || true
rm -rf .provekit/runs .provekit/witnesses 2>/dev/null || true

echo "== mint (plain-pytest + pandas.testing + pytest-witness over the project) =="
"$BIN" mint --out . --quiet

echo "== prove (consistency AND witness) =="
report="$(PATH="$VENV/bin:$PATH" "$BIN" prove . 2>/dev/null)"
echo "$report"

echo ""
echo "== self-check: provekit must prove the good code and refuse the bug both ways =="
fail=0
check() { if echo "$report" | grep -q "$2"; then echo "  ok: $1"; else echo "  MISSING: $1 ($2)"; fail=1; fi; }
# Consistency axis: the two good contracts discharge, the contradiction is UNSAT.
check "consistency discharges Series.sum == 6"      "consistent about callsite .test_column_sum_is_six"
check "consistency discharges frame round-trip"     "consistent about callsite .test_frame_round_trips_exactly"
check "consistency REFUSES the contradiction"       "contradictory about callsite .test_column_sum_contradiction"
# Witness axis: ONE WitnessPackageMemento over the suite. The package reproduces;
# the good tests passed IN it, the contradictory test failed -- so the package is
# refused, naming the failing test. The per-test facts live in the package.
check "witness package reproduces"                  "bundle reproduced"
check "witness package names the failing test"      "test_column_sum_contradiction"
# read the per-test outcomes straight from the content-addressed package
"$VENV/bin/python" - <<'PY' || fail=1
import json, glob, sys
b = glob.glob(".provekit/witnesses/*.witness")
if not b: print("  MISSING: witness package"); sys.exit(1)
out = {}
for line in open(b[0], "rb"):
    line = line.strip()
    if line:
        w = json.loads(line); out[w["test"].split("::")[-1]] = w["outcome"]
ok  = out.get("test_column_sum_is_six") == "passed" and out.get("test_frame_round_trips_exactly") == "passed"
bad = out.get("test_column_sum_contradiction") == "failed"
print(f"  {'ok' if ok else 'MISSING'}: package records good tests passed")
print(f"  {'ok' if bad else 'MISSING'}: package records the contradiction failed")
sys.exit(0 if (ok and bad) else 1)
PY

echo ""
if [ "$fail" -eq 0 ]; then
  echo "PASS: pandas proved correct (both axes); the contradictory test refused both ways."
else
  echo "FAIL: provekit did not produce the expected verdict."; exit 1
fi
