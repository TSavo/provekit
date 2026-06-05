#!/usr/bin/env bash
# The numpy showcase showdown: all the verbs over one operation, numpy.add.
#
#   lift        — the numpy sugar .proof (imports/) + numpy.testing mints the
#                 CONTRACT + pytest-witness RUNS the test for the WITNESS.
#   materialize — a @boundary(numpy.add) stub gets its body filled with the
#                 sugar body_text (from the .proof, kit-side).
#   recognize   — a production np.add callsite is found from the sugar .proof
#                 (alias-resolved, anywhere).
#   prove       — the contract discharges two ways: CONSISTENT (numpy.testing
#                 z3) AND WITNESSED (pytest-witness re-run by recompute).
#   degenerate  — a contradictory contract (np.add == 5 AND == 6) is REFUSED by
#                 BOTH consistency (z3 UNSAT) and the witness (run says 'failed').
#
# Everything is kit-side; the .proof is the transport; rust stays proof-blind.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BIN="$REPO/implementations/rust/target/debug/provekit"
PP="$REPO/implementations/python/provekit-lift-python-source/src:$REPO/implementations/python/provekit-lift-py-tests/src"

# The pytest-witness lifter RUNS numpy's test, so it needs numpy + the kit deps
# in a venv (PEP 668: never --break-system-packages). The witness manifest
# points its command/discharge_command at this venv python.
VENV="${NUMPY_WITNESS_VENV:-/tmp/numpy-witness-venv}"
if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q numpy pytest pynacl \
    -e "$REPO/implementations/python/provekit-lift-py-tests" \
    -e "$REPO/implementations/python/provekit-lift-python-source" \
    -e "$REPO/implementations/python/provekit-lift-py-pytest-witness"
fi

cd "$HERE"
rm -f blake3-512:*.proof 2>/dev/null || true
rm -rf .provekit/runs .provekit/witnesses 2>/dev/null || true

echo "== materialize @boundary(numpy.add) =="
PYTHONPATH="$PP" python3 -c "
from provekit_lift_python_source.bind_rpc import dispatch
r=dispatch({'jsonrpc':'2.0','id':1,'method':'provekit.plugin.materialize','params':{'project_root':'.','source_paths':['boundary.py'],'write':True}})
print(r['result']['results'][0]['outcome'], r['result']['results'][0].get('materialized'))"

echo "== recognize np.add in app.py =="
"$BIN" recognize --surface python-bind --target python --project . --source app.py --json 2>/dev/null \
  | python3 -c "import sys,json;[print('recognized',t['symbol'],'tier',t['match_tier']) for t in json.load(sys.stdin)['tags']]"

echo "== mint (contract + witness) + prove =="
"$BIN" mint --out . >/dev/null
"$BIN" prove .
