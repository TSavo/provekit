#!/usr/bin/env bash
# Unit tests for JavaTestAssertionsRpc
# Compiles the kit, drives it via JSON-RPC, asserts on output with python3.
# Skips cleanly if no JDK is on PATH.
set -euo pipefail

command -v javac >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java  >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP: no python3 on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KIT="$(cd "$HERE/.." && pwd)"
OUT="$KIT/out"
FIXTURES="$HERE/fixtures"

echo "== build kit =="
bash "$KIT/build.sh" "$OUT" >/dev/null 2>&1

JAVA_RPC="java \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED \
  -cp $OUT JavaTestAssertionsRpc"

run_lift() {
  local fixture_file="$1"
  local fixture_dir
  fixture_dir="$(dirname "$fixture_file")"
  local fixture_name
  fixture_name="$(basename "$fixture_file")"
  # Emit JSON-RPC initialize + lift + shutdown
  python3 - "$fixture_dir" "$fixture_name" <<'PY'
import sys, json
fixture_dir, fixture_file = sys.argv[1], sys.argv[2]
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
print(json.dumps({"jsonrpc":"2.0","id":2,"method":"lift","params":{
    "workspace_root": fixture_dir,
    "source_paths": [fixture_file],
}}))
print(json.dumps({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}))
PY
}

# ──────────────────────────────────────────────────────────────
# TEST 1: exact-lift — ExactLift.java should produce 2 contracts
# ──────────────────────────────────────────────────────────────
echo
echo "-- test 1: exact-lift --"
RESULT1="$(run_lift "$FIXTURES/ExactLift.java" | eval "$JAVA_RPC" 2>/dev/null)"
python3 - "$RESULT1" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
# Find the lift response (id=2)
lift_resp = None
for line in lines:
    if not line.strip():
        continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response found"
result = lift_resp["result"]
assert result["kind"] == "ir-document", f"unexpected kind: {result['kind']}"
ir = result["ir"]
assert len(ir) == 2, f"expected 2 contracts, got {len(ir)}: {json.dumps(ir, indent=2)}"

# Check g#euf# contract
g_contract = next((c for c in ir if "g#euf#" in c["name"]), None)
assert g_contract is not None, f"no g#euf# contract in {[c['name'] for c in ir]}"
assert g_contract["name"] == "g#euf#c:callresult_g_a1(i:2)::assertion", \
    f"wrong name: {g_contract['name']}"
assert g_contract["outBinding"] == "out", f"wrong outBinding: {g_contract['outBinding']}"
inv = g_contract["inv"]
assert inv["kind"] == "and", f"inv.kind={inv['kind']}"
atomic = inv["operands"][0]
assert atomic["kind"] == "atomic", f"atomic.kind={atomic['kind']}"
assert atomic["name"] == "=", f"atomic.name={atomic['name']}"
ctor = atomic["args"][0]
assert ctor["kind"] == "ctor", f"ctor.kind={ctor['kind']}"
assert ctor["name"] == "call:g", f"ctor.name={ctor['name']}"
assert ctor["args"][0] == {"kind":"const","value":2,"sort":{"kind":"primitive","name":"Int"}}, \
    f"ctor.args[0]={ctor['args'][0]}"
expected_const = atomic["args"][1]
assert expected_const == {"kind":"const","value":2,"sort":{"kind":"primitive","name":"Int"}}, \
    f"expected const={expected_const}"

# Check h#euf# contract (2 args, negative)
h_contract = next((c for c in ir if "h#euf#" in c["name"]), None)
assert h_contract is not None, f"no h#euf# contract in {[c['name'] for c in ir]}"
assert h_contract["name"] == "h#euf#c:callresult_h_a2(i:-1,i:3)::assertion", \
    f"wrong name: {h_contract['name']}"

print("PASS: exact-lift: 2 contracts, correct IR shape")
PY

# ──────────────────────────────────────────────────────────────
# TEST 2: discrimination — string-literal arg MUST be refused, NOT lifted
# ──────────────────────────────────────────────────────────────
echo
echo "-- test 2: discrimination (string-arg refused by name) --"
RESULT2="$(run_lift "$FIXTURES/StringArgDiscrimination.java" | eval "$JAVA_RPC" 2>/dev/null)"
python3 - "$RESULT2" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip():
        continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
# No contracts should be emitted (string literal and var are not int literals)
assert len(ir) == 0, f"expected 0 contracts from string-arg file, got {len(ir)}: {json.dumps(ir)}"
# At least 1 diagnostic for the string case
assert len(diags) >= 1, f"expected >=1 diagnostics, got {json.dumps(diags)}"
kinds = [d.get("kind") for d in diags]
assert all(k == "lift-gap" for k in kinds), f"unexpected diag kinds: {kinds}"
reasons = [d.get("reason","") for d in diags]
print(f"PASS: discrimination: 0 contracts, {len(diags)} named refusals: {reasons}")
PY

# ──────────────────────────────────────────────────────────────
# TEST 3: structural proof — string-in-literal and comment NOT lifted
# ──────────────────────────────────────────────────────────────
echo
echo "-- test 3: structural proof (string literal + comment not lifted) --"
RESULT3="$(run_lift "$FIXTURES/StructuralProof.java" | eval "$JAVA_RPC" 2>/dev/null)"
python3 - "$RESULT3" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip():
        continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
# ONLY the real @Test assertion should be lifted (g(7)==7)
# g(1) from string literal and g(3) from comment must NOT appear
names = [c["name"] for c in ir]
assert len(ir) == 1, f"expected exactly 1 contract (only real @Test), got {len(ir)}: {names}"
assert ir[0]["name"] == "g#euf#c:callresult_g_a1(i:7)::assertion", \
    f"wrong contract: {ir[0]['name']}"
# Crucially, no g(1) or g(3) contract
assert not any("i:1" in n for n in names), \
    f"string-literal g(1) was lifted (string scanner bug!): {names}"
assert not any("i:3" in n for n in names), \
    f"comment g(3) was lifted (string scanner bug!): {names}"
print(f"PASS: structural: only real @Test contract lifted, string literal and comment ignored")
PY

echo
echo "== all tests PASS =="
