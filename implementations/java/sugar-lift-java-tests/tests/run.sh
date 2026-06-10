#!/usr/bin/env bash
# Unit tests for JavaTestAssertionsRpc (Phase 3: final-oracle + loop→∀ lifter)
# Compiles the kit, drives it via JSON-RPC, asserts on output with python3.
# Skips cleanly if no JDK is on PATH.
#
# Test suite (P1/P2):
#   1. Vocab derivation + exact-lift: ExactLift.java produces 2 contracts with
#      correct IR shape (assertEquals=equality, expected-first from learned vocab)
#   2. Discrimination (string-arg): StringArgDiscrimination.java → 0 contracts,
#      named refusals
#   3. Structural proof: StructuralProof.java → only real @Test lifted, not
#      string literals or comments
#   4. Delta discrimination: DeltaApprox.java → 0 contracts, named refusal citing
#      "approximate assertion (delta)" (the false-pass guard)
#   5. No-vocab-configured: ExactLiftNoVocab.java with no assertion_source_dirs →
#      0 contracts, named refusal ("no learned vocabulary") — hardcode is gone
#   6. Extended vocab (assertNotEquals, assertNull, assertNotNull): produces correct
#      ≠ and None-ctor IR atoms
#   7. Exceptions overlay: assertArrayEquals in equality via .sugar/vocab-exceptions/
#
# Test suite (P3 — loop→∀ + final-oracle):
#   8. Loop→∀ exact shape: ForLoopForall.java → 1 forall contract with
#      correct IR (forall x. 0<=x<3 => g(x)==1) and ::loop::x name
#   9. Accumulator discrimination: ForLoopAccumulator.java → loop REFUSED by name
#      (acc += g(x) is a mutation → not a universal)
#  10. Effectively-final positive: ForLoopEffectivelyFinal.java → 1 forall contract
#      (outer non-final but never-reassigned local does NOT block lifting)
#  11. Non-literal bound discrimination: ForLoopOpenBound.java → loop REFUSED
#      (x < n where n is a variable → open forall)
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

JAVA_CMD="java \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED \
  -cp $OUT JavaTestAssertionsRpc"

# run_lift <workspace_root> <source_path_relative_to_workspace>
# Sends initialize + lift(workspace_root, [source_path]) + shutdown to the kit.
run_lift() {
  local workspace_root="$1"
  local source_path="$2"
  python3 - "$workspace_root" "$source_path" <<'PY'
import sys, json
workspace_root, source_path = sys.argv[1], sys.argv[2]
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
print(json.dumps({"jsonrpc":"2.0","id":2,"method":"lift","params":{
    "workspace_root": workspace_root,
    "source_paths": [source_path],
}}))
print(json.dumps({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}))
PY
}

# extract_lift_result <json_lines>: print the lift response result as JSON
extract_lift() {
  python3 - "$1" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        print(json.dumps(obj["result"]))
        sys.exit(0)
sys.exit(1)
PY
}

# ──────────────────────────────────────────────────────────────
# TEST 1: vocab derivation + exact-lift
# Workspace: fixtures/ (has .sugar/config.toml pointing at vendor/junit5/)
# ExactLift.java uses org.junit.Assert.assertEquals (JUnit 4 form).
# VocabDeriver learns assertEquals=equality from the vendored JUnit5 Assertions.java.
# Expected: 2 contracts with correct IR (= relation, expected-first from learned vocab).
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 1: vocab derivation + exact-lift (2 contracts expected)"
echo "────────────────────────────────────────────────────────────────"
RESULT1="$(run_lift "$FIXTURES" "ExactLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT1" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response found"
result = lift_resp["result"]
assert result["kind"] == "ir-document", f"unexpected kind: {result['kind']}"
ir = result["ir"]
diags = result["diagnostics"]
assert len(ir) == 2, f"expected 2 contracts, got {len(ir)}: {json.dumps(ir, indent=2)}\ndiags={json.dumps(diags,indent=2)}"

# Check g#euf# contract (assertEquals learned as equality, expected=arg[0])
g_contract = next((c for c in ir if "g#euf#" in c["name"]), None)
assert g_contract is not None, f"no g#euf# contract in {[c['name'] for c in ir]}"
assert g_contract["name"] == "g#euf#c:callresult_g_a1(i:2)::assertion", \
    f"wrong name: {g_contract['name']}"
assert g_contract["outBinding"] == "out"
inv = g_contract["inv"]
assert inv["kind"] == "and"
atomic = inv["operands"][0]
assert atomic["kind"] == "atomic"
assert atomic["name"] == "=", f"relation should be = not {atomic['name']}"
ctor = atomic["args"][0]
assert ctor["kind"] == "ctor" and ctor["name"] == "call:g"
assert ctor["args"][0] == {"kind":"const","value":2,"sort":{"kind":"primitive","name":"Int"}}
const_val = atomic["args"][1]
assert const_val == {"kind":"const","value":2,"sort":{"kind":"primitive","name":"Int"}}, \
    f"expected const 2 but got {const_val}"

# Check h#euf# contract (2-arg call, negative literal)
h_contract = next((c for c in ir if "h#euf#" in c["name"]), None)
assert h_contract is not None, f"no h#euf# contract in {[c['name'] for c in ir]}"
assert h_contract["name"] == "h#euf#c:callresult_h_a2(i:-1,i:3)::assertion", \
    f"wrong name: {h_contract['name']}"

print("PASS: vocab derivation + exact-lift: 2 contracts, correct IR shape (= relation, expected-first)")
PY

# ──────────────────────────────────────────────────────────────
# TEST 2: discrimination — string-literal arg refused, not lifted
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 2: discrimination (string-arg refused by name)"
echo "────────────────────────────────────────────────────────────────"
RESULT2="$(run_lift "$FIXTURES" "StringArgDiscrimination.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT2" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
assert len(ir) == 0, f"expected 0 contracts from string-arg file, got {len(ir)}: {json.dumps(ir)}"
assert len(diags) >= 1, f"expected >=1 diagnostics, got {json.dumps(diags)}"
reasons = [d.get("reason","") for d in diags]
print(f"PASS: discrimination: 0 contracts, {len(diags)} named refusals: {reasons}")
PY

# ──────────────────────────────────────────────────────────────
# TEST 3: structural proof — string-in-literal and comment NOT lifted
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 3: structural proof (string literal + comment not lifted)"
echo "────────────────────────────────────────────────────────────────"
RESULT3="$(run_lift "$FIXTURES" "StructuralProof.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT3" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
names = [c["name"] for c in ir]
assert len(ir) == 1, f"expected exactly 1 contract (only real @Test), got {len(ir)}: {names}"
assert ir[0]["name"] == "g#euf#c:callresult_g_a1(i:7)::assertion", \
    f"wrong contract: {ir[0]['name']}"
assert not any("i:1" in n for n in names), \
    f"string-literal g(1) was lifted (string scanner bug!): {names}"
assert not any("i:3" in n for n in names), \
    f"comment g(3) was lifted (string scanner bug!): {names}"
print(f"PASS: structural: only real @Test contract lifted; string literal and comment ignored")
PY

# ──────────────────────────────────────────────────────────────
# TEST 4: delta discrimination — approximate assertion REFUSED by name
# assertEquals(1.0f, f(2), 0.5f) has a delta parameter in JUnit5 source.
# VocabDeriver must classify this overload as APPROXIMATE.
# At lift time: 0 contracts, 1 refusal naming "approximate assertion (delta)".
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 4: delta discrimination (approximate assertion refused)"
echo "────────────────────────────────────────────────────────────────"
RESULT4="$(run_lift "$FIXTURES" "DeltaApprox.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT4" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
assert len(ir) == 0, \
    f"delta-approximate assertEquals must produce ZERO contracts, got {len(ir)}: {json.dumps(ir)}"
assert len(diags) >= 1, \
    f"expected at least 1 refusal diagnostic, got {json.dumps(diags)}"
reasons = [d.get("reason","") for d in diags]
approx_diags = [r for r in reasons if "approximate" in r.lower() or "delta" in r.lower()]
assert len(approx_diags) >= 1, \
    f"expected a diagnostic naming 'approximate' or 'delta', got: {reasons}"
print(f"PASS: delta discrimination: 0 contracts, refusal: {approx_diags}")
PY

# ──────────────────────────────────────────────────────────────
# TEST 5: no-vocab-configured — the hardcode is gone
# Workspace: fixtures/no-vocab/ (NO .sugar/config.toml).
# assertEquals must be refused by name ("no learned vocabulary").
# Zero contracts expected. Proves the hardcode was removed.
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 5: no-vocab-configured (hardcode is gone — assertEquals refused)"
echo "────────────────────────────────────────────────────────────────"
NOVOCAB_DIR="$FIXTURES/no-vocab"
RESULT5="$(run_lift "$NOVOCAB_DIR" "ExactLiftNoVocab.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT5" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
assert len(ir) == 0, \
    f"with no vocab configured, assertEquals must produce ZERO contracts (hardcode gone). Got {len(ir)}: {json.dumps(ir)}"
assert len(diags) >= 1, \
    f"expected at least 1 named refusal, got {json.dumps(diags)}"
reasons = [d.get("reason","") for d in diags]
vocab_diags = [r for r in reasons if "no learned vocabulary" in r or "vocabulary" in r.lower() or "refused" in r.lower()]
assert len(vocab_diags) >= 1, \
    f"expected a diagnostic naming 'no learned vocabulary' or 'refused', got: {reasons}"
print(f"PASS: no-vocab: 0 contracts, named refusal: {vocab_diags}")
PY

# ──────────────────────────────────────────────────────────────
# TEST 6: extended vocab — assertNotEquals (≠), assertNull (None), assertNotNull (≠None)
# Workspace: fixtures/ (has config.toml → vendored junit5)
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 6: extended vocab (assertNotEquals=≠, assertNull/assertNotNull=None-ctor)"
echo "────────────────────────────────────────────────────────────────"
RESULT6="$(run_lift "$FIXTURES" "NotEqualsAndNull.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT6" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]

names = [c["name"] for c in ir]
print(f"  contracts: {names}")
print(f"  diagnostics: {[d.get('reason','') for d in diags]}")

# assertNotEquals(2, g(2)) → ≠ relation
g_neq = next((c for c in ir if "g#euf#" in c["name"]), None)
assert g_neq is not None, f"no g#euf# contract from assertNotEquals in {names}"
inv_g = g_neq["inv"]
atomic_g = inv_g["operands"][0]
assert atomic_g["name"] == "≠", \
    f"assertNotEquals should produce ≠ relation, got {atomic_g['name']}"
# const value should be 2 (the unexpected value)
const_g = atomic_g["args"][1]
assert const_g["value"] == 2, f"expected unexpected=2, got {const_g}"

# assertNull(getNull(5)) → =(call, None)
null_c = next((c for c in ir if "getNull#euf#" in c["name"]), None)
assert null_c is not None, f"no getNull#euf# contract from assertNull in {names}"
inv_null = null_c["inv"]
atomic_null = inv_null["operands"][0]
assert atomic_null["name"] == "=", \
    f"assertNull should produce = relation, got {atomic_null['name']}"
none_arg = atomic_null["args"][1]
assert none_arg == {"kind":"ctor","name":"None","args":[]}, \
    f"assertNull second arg should be None ctor, got {none_arg}"

# assertNotNull(getNotNull(3)) → ≠(call, None)
notnull_c = next((c for c in ir if "getNotNull#euf#" in c["name"]), None)
assert notnull_c is not None, f"no getNotNull#euf# contract from assertNotNull in {names}"
inv_nn = notnull_c["inv"]
atomic_nn = inv_nn["operands"][0]
assert atomic_nn["name"] == "≠", \
    f"assertNotNull should produce ≠ relation, got {atomic_nn['name']}"
none_arg2 = atomic_nn["args"][1]
assert none_arg2 == {"kind":"ctor","name":"None","args":[]}, \
    f"assertNotNull second arg should be None ctor, got {none_arg2}"

print(f"PASS: extended vocab: assertNotEquals=≠, assertNull==(None), assertNotNull=≠(None)")
PY

# ──────────────────────────────────────────────────────────────
# TEST 7: exceptions overlay — assertArrayEquals classified as equality via override
# The .sugar/vocab-exceptions/org.junit.jupiter.api.Assertions.json file
# already overrides assertArrayEquals into equality. Verify it loads.
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 7: exceptions overlay (assertArrayEquals in equality via override)"
echo "────────────────────────────────────────────────────────────────"
# We verify that the vocab includes assertArrayEquals in equality
# by sending a vocab-derivation diagnostic check.
# We use a synthetic fixture inline that calls assertArrayEquals.
ARRAYEQ_FIXTURE="$(cat <<'JAVA'
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
public class ArrayEqTest {
    @Test public void testArr() {
        // assertArrayEquals is in "unlearned" from VocabDeriver (not in Assertions.java methods
        // that pass our equality gate), BUT the exceptions overlay adds it to equality.
        // Since the actual arg here is not a method-call-with-int-lits, it will produce
        // a diagnostic — but that diagnostic should NOT say "unlearned vocabulary".
        // It should say "second arg is not a method call" (i.e. it WAS classified as equality).
        assertArrayEquals(new int[]{1}, getArr(2));
    }
    private int[] getArr(int x) { return new int[]{x}; }
}
JAVA
)"
TMPF="$(mktemp /tmp/ArrayEqTest_XXXXXX.java)"
echo "$ARRAYEQ_FIXTURE" > "$TMPF"
RESULT7="$(python3 - "$FIXTURES" "$TMPF" <<'PY' | eval "$JAVA_CMD" 2>/dev/null
import sys, json, os
workspace_root, fixture_file = sys.argv[1], sys.argv[2]
fixture_name = os.path.basename(fixture_file)
print(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
print(json.dumps({"jsonrpc":"2.0","id":2,"method":"lift","params":{
    "workspace_root": workspace_root,
    "source_paths": [fixture_file],
}}))
print(json.dumps({"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}))
PY
)"
rm -f "$TMPF"
python3 - "$RESULT7" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None
result = lift_resp["result"]
diags = result["diagnostics"]
# The test verifies that assertArrayEquals is processed as equality (not "unlearned").
# It will fail at lift because "new int[]{1}" is not an int literal — but that
# means the override DID work (we got past the unlearned gate into the lift path).
reasons = [d.get("reason","") for d in diags]
unlearned_diags = [r for r in reasons if "unlearned" in r.lower() or "not in learned vocabulary" in r.lower()]
assert len(unlearned_diags) == 0, \
    f"assertArrayEquals should be in equality via override, not unlearned. Diags: {reasons}"
print(f"PASS: exceptions overlay: assertArrayEquals NOT in unlearned (override applied). Diags: {reasons}")
PY


echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 8: loop→∀ exact IR shape (ForLoopForall.java)"
echo "────────────────────────────────────────────────────────────────"
RESULT8="$(run_lift "$FIXTURES" "ForLoopForall.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT8" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response"
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
assert len(ir) == 1, f"expected 1 forall contract, got {len(ir)}: {json.dumps(ir,indent=2)}\ndiags={json.dumps(diags,indent=2)}"
c = ir[0]

# Name must contain ::loop::x
assert "::loop::x" in c["name"], f"contract name does not contain ::loop::x: {c['name']}"

# Shape: kind=contract, inv.kind=and, inv.operands[0].kind=forall
assert c["kind"] == "contract", f"kind: {c['kind']}"
assert c["outBinding"] == "out", f"outBinding: {c.get('outBinding')}"
inv = c["inv"]
assert inv["kind"] == "and", f"inv.kind: {inv['kind']}"
assert len(inv["operands"]) == 1, f"inv.operands count: {len(inv['operands'])}"

fa = inv["operands"][0]
assert fa["kind"] == "forall", f"forall kind: {fa['kind']}"
assert fa["name"] == "x", f"forall bound var: {fa['name']}"
assert fa["sort"] == {"kind": "primitive", "name": "Int"}, f"sort: {fa['sort']}"

body = fa["body"]
assert body["kind"] == "implies", f"implies kind: {body['kind']}"
assert len(body["operands"]) == 2, f"implies operands: {len(body['operands'])}"

guard = body["operands"][0]
assert guard["kind"] == "and", f"guard kind: {guard['kind']}"
guard_ops = guard["operands"]
assert len(guard_ops) == 2, f"guard operands count: {len(guard_ops)}"

# Lower bound: 0 ≤ x
lo = guard_ops[0]
assert lo["kind"] == "atomic" and lo["name"] == "≤", f"lower bound atom: {lo}"
assert lo["args"][0] == {"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}, f"lo.left: {lo['args'][0]}"
assert lo["args"][1] == {"kind":"var","name":"x"}, f"lo.right: {lo['args'][1]}"

# Upper bound: x < 3
hi = guard_ops[1]
assert hi["kind"] == "atomic" and hi["name"] == "<", f"upper bound atom: {hi}"
assert hi["args"][0] == {"kind":"var","name":"x"}, f"hi.left: {hi['args'][0]}"
assert hi["args"][1] == {"kind":"const","value":3,"sort":{"kind":"primitive","name":"Int"}}, f"hi.right: {hi['args'][1]}"

# Body: and([atomic(=, [ctor(call:g,[var:x]), const(1)])])
body_conj = body["operands"][1]
assert body_conj["kind"] == "and", f"body conj kind: {body_conj['kind']}"
assert len(body_conj["operands"]) == 1, f"body operands: {len(body_conj['operands'])}"
atom = body_conj["operands"][0]
assert atom["kind"] == "atomic" and atom["name"] == "=", f"body atom: {atom}"
ctor = atom["args"][0]
assert ctor["kind"] == "ctor" and ctor["name"] == "call:g", f"ctor: {ctor}"
assert ctor["args"] == [{"kind":"var","name":"x"}], f"ctor args: {ctor['args']}"
const_val = atom["args"][1]
assert const_val == {"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}}, f"const: {const_val}"

# Closedness: no free variables (the only var is x, which is the bound var)
print(f"PASS: loop→∀ IR shape correct. Contract: {c['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 9: accumulator discrimination (ForLoopAccumulator.java)"
echo "────────────────────────────────────────────────────────────────"
RESULT9="$(run_lift "$FIXTURES" "ForLoopAccumulator.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT9" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response"
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
# The loop MUST be refused — no forall contract emitted
assert len(ir) == 0, f"expected 0 contracts (loop refused), got {len(ir)}: {json.dumps(ir,indent=2)}"
# Must have a diagnostic naming the accumulator or mutation pattern
reasons = [d.get("reason","") for d in diags]
refusal_for_loop = [r for r in reasons if "accum" in r.lower() or "mutat" in r.lower() or "loop" in r.lower()]
assert len(refusal_for_loop) > 0, \
    f"expected a loop refusal diagnostic (accumulator/mutation), got: {reasons}"
print(f"PASS: accumulator loop refused by name. Reason: {refusal_for_loop[0]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 10: effectively-final positive (ForLoopEffectivelyFinal.java)"
echo "────────────────────────────────────────────────────────────────"
RESULT10="$(run_lift "$FIXTURES" "ForLoopEffectivelyFinal.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT10" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response"
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
# The loop MUST lift — the effectively-final `outer` local does not block it
assert len(ir) == 1, \
    f"expected 1 forall contract (effectively-final local does not block), got {len(ir)}: {json.dumps(ir,indent=2)}\ndiags={json.dumps(diags,indent=2)}"
assert "::loop::x" in ir[0]["name"], f"contract name: {ir[0]['name']}"
print(f"PASS: effectively-final outer local does not block loop lifting. Contract: {ir[0]['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 11: non-literal bound discrimination (ForLoopOpenBound.java)"
echo "────────────────────────────────────────────────────────────────"
RESULT11="$(run_lift "$FIXTURES" "ForLoopOpenBound.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT11" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response"
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
# Must be refused — no contract emitted
assert len(ir) == 0, f"expected 0 contracts (open bound refused), got {len(ir)}: {json.dumps(ir,indent=2)}"
reasons = [d.get("reason","") for d in diags]
open_bound_diags = [r for r in reasons if "open" in r.lower() or "literal" in r.lower() or "bound" in r.lower()]
assert len(open_bound_diags) > 0, \
    f"expected an open-bound refusal diagnostic, got: {reasons}"
print(f"PASS: non-literal bound refused by name. Reason: {open_bound_diags[0]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 12: loop-variable mutation discrimination (ForLoopVarMutation.java)"
echo "────────────────────────────────────────────────────────────────"
RESULT12="$(run_lift "$FIXTURES" "ForLoopVarMutation.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT12" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
lift_resp = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        lift_resp = obj
        break
assert lift_resp is not None, "no lift response"
result = lift_resp["result"]
ir = result["ir"]
diags = result["diagnostics"]
# g(x++) mutates the loop variable inside the body: the iteration space is
# not the stated range. Must be refused by the LOOP-VARIABLE gate itself,
# not merely the arg-shape gate (defense must hold if arg shapes widen).
assert len(ir) == 0, f"expected 0 contracts (loop-var mutation refused), got {len(ir)}: {json.dumps(ir,indent=2)}"
reasons = [d.get("reason","") for d in diags]
loopvar_diags = [r for r in reasons if "mutates the loop variable" in r]
assert len(loopvar_diags) > 0, \
    f"expected the loop-variable mutation refusal, got: {reasons}"
print(f"PASS: loop-variable body mutation refused by its own gate. Reason: {loopvar_diags[0]}")
PY

echo
echo "== all 12 tests PASS =="
