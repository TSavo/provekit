#!/usr/bin/env bash
# Unit tests for JavaTestAssertionsRpc (Phase 4: TestNG support — learned vocab per-framework)
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
#
# Test suite (H1 — hardening rollup: discrimination fixtures for each fix):
#  31. [A1] cross-class ambiguity: helperChk in 2 classes → UNLEARNED, not first-match
#  32a. [A2] wildcard static import: import static org.junit.Assert.* → assertEquals lifts
#  32b. [A2] non-framework wildcard: import static com.example.* → silent skip (not bound)
#  33. [A3] user-scope impostor: assertEquals without static import → silent skip (not lift)
#  34. [C8] TestNG assertNotEquals 2-arg: INEQUALITY, not APPROX (delta overload blocked)
#  35. [B6] lineLength=0 → encodeNoChunk universe registered; lineLength=76 → refused
#
# Test suite (G2b — comparison-bound lifting):
#  41. assertTrue(g(7) < 10) → <(call:g(7), 10) with correct #euf# name
#  42. assertTrue(5 > g(2)) → <(call:g(2), 5) [lit-left mirror normalised]
#  43. assertFalse(g(2) < 5) → >=(call:g(2), 5) [negated predicate]
#  44. non-literal bound refused; both-calls refused (both named)
#  45. #euf# name matches assertEquals schema → federation is live
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

# ──────────────────────────────────────────────────────────────
# Test suite (P4 — TestNG support: proof that vocab must be learned per-framework):
#  13. TestNG vocab derivation: derived table shows assertEquals=equality with
#      ACTUAL-FIRST order (expectedArgIndex=1); explicit assertion on learned order.
#  14. TestNG exact-lift: Assert.assertEquals(g(2), 1) in a TestNG file →
#      same contract name/IR as JUnit assertEquals(1, g(2)).
#  15. ORDER DISCRIMINATION: same source text "assertEquals(g(2), 1)" in a
#      JUnit-imports file → REFUSED (arg[0]=g(2) is not an int literal under
#      JUnit order); in TestNG-imports file → LIFTS. Two fixtures, same text,
#      opposite outcomes — this asymmetry IS the proof.
#  16. Dual-import ambiguity: imports both org.junit.Assert and org.testng.Assert
#      → named refusal on all assertions ("ambiguous assertion vocabulary").
#  17. TestNG delta overload: Assert.assertEquals(g_d(2), 1.0, 0.5) →
#      delta param → APPROXIMATE → named refusal.
#  18. assertThat → unlearned named refusal (not in any vendored source vocab).
#  19. TestNG @Test annotation recognized: a file using org.testng.annotations.Test
#      has its @Test methods lifted (same as JUnit @Test).
# ──────────────────────────────────────────────────────────────

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 13: TestNG vocab derivation (actual-first order learned)"
echo "────────────────────────────────────────────────────────────────"
# Drive a vocab-only diagnostic: lift an empty file in the fixtures workspace,
# then ask for a derivation probe via a synthetic fixture that reports the vocab.
# We prove the order by lifting a TestNG fixture and checking the IR: if the
# VocabDeriver learned actual-first, then Assert.assertEquals(g(2), 1) in a
# TestNG file should produce =(call:g(2), 1) with the CALL in arg[0] of the
# atomic (the actual) and 1 in arg[1] (the expected constant).
# We verify: 1 contract produced, const=1, call=g(2) — this proves actual-first.
RESULT13="$(run_lift "$FIXTURES" "TestNGExactLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT13" <<'PY'
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
assert len(ir) == 1, \
    f"expected 1 contract (TestNG actual-first lift), got {len(ir)}: {json.dumps(ir,indent=2)}\ndiags={json.dumps(diags,indent=2)}"
c = ir[0]
assert c["name"] == "g#euf#c:callresult_g_a1(i:2)::assertion", \
    f"wrong contract name: {c['name']}"
atomic = c["inv"]["operands"][0]
assert atomic["name"] == "=", f"relation must be ="
# The ctor (call:g(2)) must be in arg[0], const 1 in arg[1]
ctor = atomic["args"][0]
assert ctor["kind"] == "ctor" and ctor["name"] == "call:g", \
    f"arg[0] must be call:g ctor, got: {ctor}"
const_node = atomic["args"][1]
assert const_node == {"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}}, \
    f"arg[1] (expected/const) must be 1, got: {const_node}"
# This proves: VocabDeriver learned actual-first (param[0]="actual") from TestNG source.
# The constant (expected) was at arg[1]=1, not arg[0]=g(2). Actual-first confirmed.
print("PASS: TestNG vocab derivation: assertEquals=equality, actual-first order learned "
      "(param[0]='actual' → expectedArgIndex=1; const 1 correctly extracted from arg[1])")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 14: TestNG exact-lift IR byte-identity with JUnit"
echo "────────────────────────────────────────────────────────────────"
# TestNG: Assert.assertEquals(g(2), 1)  → same IR as JUnit: assertEquals(1, g(2))
# Both must produce IDENTICAL contract name and IDENTICAL inv structure.
# The JUnit reference is from ExactLift.java (test 1).
RESULT14_TESTNG="$(run_lift "$FIXTURES" "TestNGExactLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
RESULT14_JUNIT="$(run_lift "$FIXTURES" "JUnitEqualityRef.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT14_TESTNG" "$RESULT14_JUNIT" <<'PY'
import sys, json
def get_lift_result(raw):
    lines = raw.strip().split('\n')
    for line in lines:
        if not line.strip(): continue
        obj = json.loads(line)
        if obj.get("id") == 2:
            return obj["result"]
    raise AssertionError("no lift response")

testng_result = get_lift_result(sys.argv[1])
junit_result = get_lift_result(sys.argv[2])

# Find the g#euf#c:callresult_g_a1(i:2)::assertion contract in each
testng_g = next((c for c in testng_result["ir"] if "g#euf#c:callresult_g_a1(i:2)" in c["name"]), None)
junit_g  = next((c for c in junit_result["ir"]  if "g#euf#c:callresult_g_a1(i:2)" in c["name"]), None)
assert testng_g is not None, f"no g#euf# in TestNG IR: {[c['name'] for c in testng_result['ir']]}"
assert junit_g  is not None, f"no g#euf# in JUnit IR: {[c['name'] for c in junit_result['ir']]}"

# Name must be identical
assert testng_g["name"] == junit_g["name"], \
    f"contract names differ: TestNG={testng_g['name']}  JUnit={junit_g['name']}"

# inv must be identical (byte-for-byte after JSON normalization)
testng_inv_str = json.dumps(testng_g["inv"], sort_keys=True)
junit_inv_str  = json.dumps(junit_g["inv"],  sort_keys=True)
assert testng_inv_str == junit_inv_str, \
    f"IR inv differs:\n  TestNG: {testng_inv_str}\n  JUnit:  {junit_inv_str}"

print(f"PASS: TestNG exact-lift IR is byte-identical to JUnit IR for the same claim.")
print(f"      Contract: {testng_g['name']}")
print(f"      TestNG source: Assert.assertEquals(g(2), 1)  [actual-first]")
print(f"      JUnit  source: assertEquals(1, g(2))  [expected-first]")
print(f"      Both produce: =(call:g(2), 1)  — same contract, same CID-able shape")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 15: ORDER DISCRIMINATION — same text, opposite outcomes"
echo "────────────────────────────────────────────────────────────────"
# JUnitOrderDiscrimination.java: assertEquals(g(2), 1) with JUnit imports
# → JUnit order: arg[0]=expected; arg[0]=g(2) is NOT an int literal → REFUSED.
#
# TestNGExactLift.java: Assert.assertEquals(g(2), 1) with TestNG imports
# → TestNG order: arg[0]=actual; const=arg[1]=1 → LIFTS.
RESULT15_JUNIT="$(run_lift "$FIXTURES" "JUnitOrderDiscrimination.java" | eval "$JAVA_CMD" 2>/dev/null)"
RESULT15_TESTNG="$(run_lift "$FIXTURES" "TestNGExactLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT15_JUNIT" "$RESULT15_TESTNG" <<'PY'
import sys, json
def get_lift_result(raw):
    lines = raw.strip().split('\n')
    for line in lines:
        if not line.strip(): continue
        obj = json.loads(line)
        if obj.get("id") == 2:
            return obj["result"]
    raise AssertionError("no lift response")

junit_result  = get_lift_result(sys.argv[1])
testng_result = get_lift_result(sys.argv[2])

# JUnit file: assertEquals(g(2), 1) — JUnit order reads arg[0]=g(2) as expected.
# g(2) is NOT an int literal → refused.
assert len(junit_result["ir"]) == 0, \
    f"JUnit file: assertEquals(g(2),1) MUST be refused under JUnit order, but got contracts: {[c['name'] for c in junit_result['ir']]}"
junit_reasons = [d["reason"] for d in junit_result["diagnostics"]]
assert any("not an int literal" in r or "refused" in r.lower() for r in junit_reasons), \
    f"JUnit file: expected a refusal diagnostic, got: {junit_reasons}"

# TestNG file: Assert.assertEquals(g(2), 1) — TestNG order reads arg[0]=g(2) as actual.
# arg[1]=1 is the expected constant → LIFTS.
assert len(testng_result["ir"]) == 1, \
    f"TestNG file: assertEquals(g(2),1) MUST lift under TestNG order, but got {len(testng_result['ir'])} contracts"

print("PASS: ORDER DISCRIMINATION — same source text 'assertEquals(g(2), 1)':")
print(f"  JUnit-imports  file → REFUSED  (arg[0]=g(2) is not an int literal under JUnit order)")
print(f"  TestNG-imports file → LIFTS    (arg[0]=actual=g(2), arg[1]=expected=1 → =(call:g(2),1))")
print(f"  JUnit refusal reason: {junit_reasons[0][:80]}")
print(f"  This asymmetry IS the proof that vocab must be learned per-framework.")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 16: dual-import ambiguity → named refusal"
echo "────────────────────────────────────────────────────────────────"
RESULT16="$(run_lift "$FIXTURES" "DualImportAmbiguity.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT16" <<'PY'
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
# Both frameworks imported: vocabulary is ambiguous → named refusal, no contracts
assert len(ir) == 0, \
    f"dual-import ambiguity must produce ZERO contracts; got: {[c['name'] for c in ir]}"
assert len(diags) >= 1, f"expected at least 1 named refusal, got {diags}"
reasons = [d.get("reason","") for d in diags]
ambig_diags = [r for r in reasons if "ambiguous" in r.lower() or "both" in r.lower()]
assert len(ambig_diags) >= 1, \
    f"expected an 'ambiguous' or 'both' refusal; got: {reasons}"
print(f"PASS: dual-import ambiguity → named refusal: {ambig_diags[0][:90]}")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 17: TestNG delta overload → approximate refusal"
echo "────────────────────────────────────────────────────────────────"
RESULT17="$(run_lift "$FIXTURES" "TestNGDeltaApprox.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT17" <<'PY'
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
assert len(ir) == 0, \
    f"TestNG delta: expected 0 contracts, got {len(ir)}: {json.dumps(ir,indent=2)}"
reasons = [d.get("reason","") for d in diags]
approx_diags = [r for r in reasons if "approximate" in r.lower() or "delta" in r.lower()]
assert len(approx_diags) >= 1, \
    f"expected approximate/delta refusal diagnostic; got: {reasons}"
print(f"PASS: TestNG delta overload → approximate refusal: {approx_diags[0][:90]}")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 18: assertThat → unlearned named refusal"
echo "────────────────────────────────────────────────────────────────"
RESULT18="$(run_lift "$FIXTURES" "AssertThatUnlearned.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT18" <<'PY'
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
assert len(ir) == 0, \
    f"assertThat: expected 0 contracts, got {len(ir)}: {json.dumps(ir,indent=2)}"
reasons = [d.get("reason","") for d in diags]
unlearned_diags = [r for r in reasons if "vocabulary" in r.lower() or "refused" in r.lower() or "unlearned" in r.lower()]
assert len(unlearned_diags) >= 1, \
    f"expected 'vocabulary'/'refused'/'unlearned' refusal for assertThat; got: {reasons}"
print(f"PASS: assertThat → unlearned named refusal: {unlearned_diags[0][:90]}")
PY

# ──────────────────────────────────────────────────────────────
# Test suite (P4.5 — throw-locus derivation: the name never enters into it)
# ──────────────────────────────────────────────────────────────

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 20: DELETE-THE-KEYS — no name-keyed classification in the deriver"
echo "────────────────────────────────────────────────────────────────"
if grep -rn "isAssertEqualsName\|isAssertTrueName\|isAssertNullName\|isAssertNotEqualsName\|isAssertFalseName\|isAssertNotNullName" "$KIT/src/"; then
  echo "FAIL: name-keyed classification predicates still present in src/"
  exit 1
fi
echo "PASS: delete-the-keys — grep for name-keyed predicates returns nothing"

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 21: RENAMED-COPY discrimination — assertEquals's body under the name 'check' LIFTS"
echo "────────────────────────────────────────────────────────────────"
# fixtures/renamed-copy/framework/CheckAssert.java: method `check(expected, actual)`
# with assertEquals's guarded-throw body. Throw-locus derivation must classify
# EQUALITY; check(1, g(2)) must LIFT. The spelling never mattered.
RESULT21="$(run_lift "$FIXTURES/renamed-copy" "RenamedCopy.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT21" <<'PY'
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
assert len(ir) == 1, \
    f"renamed-copy: expected 1 contract from check(1, g(2)), got {len(ir)}: {json.dumps(ir,indent=2)}\ndiags={json.dumps(diags,indent=2)}"
c = ir[0]
inv = c["inv"]
atomic = inv["operands"][0] if inv.get("kind") == "and" else inv
assert atomic["kind"] == "atomic" and atomic["name"] == "=", \
    f"expected = atomic, got {json.dumps(atomic)[:120]}"
print(f"PASS: RENAMED-COPY — `check` (assertEquals's body, different name) classified EQUALITY and LIFTED.")
print(f"      Contract: {c['name']}")
print(f"      The name never entered into it: classification came from the throw-guard.")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 22: NAME-IMPOSTOR discrimination — assertEquals with 'return;' body REFUSED"
echo "────────────────────────────────────────────────────────────────"
# fixtures/name-impostor/framework/FakeAssert.java: method NAMED assertEquals
# whose body is `return;` — no throw locus → NOT an assertion. A lift here
# would be the falsePass.
RESULT22="$(run_lift "$FIXTURES/name-impostor" "NameImpostor.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT22" <<'PY'
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
assert len(ir) == 0, \
    f"NAME-IMPOSTOR FALSEPASS: assertEquals with `return;` body LIFTED {len(ir)} contract(s): {json.dumps(ir,indent=2)}"
reasons = [d.get("reason","") for d in diags]
locus_diags = [r for r in reasons if "no throw locus" in r.lower()]
assert len(locus_diags) >= 1, \
    f"expected named 'no throw locus' refusal; got: {reasons}"
print(f"PASS: NAME-IMPOSTOR — assertEquals with `return;` body is NOT an assertion.")
print(f"      Refusal: {locus_diags[0]}")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 23: guard-position vs param-name CROSS-CHECK disagreement → unlearned"
echo "────────────────────────────────────────────────────────────────"
# fixtures/cross-check/framework/DisagreeAssert.java: guard says expected-first
# (left operand of `actual != other`), param names say actual-first → UNLEARNED.
RESULT23="$(run_lift "$FIXTURES/cross-check" "CrossCheck.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT23" <<'PY'
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
assert len(ir) == 0, \
    f"cross-check: expected 0 contracts (order untrustworthy), got {len(ir)}: {json.dumps(ir,indent=2)}"
reasons = [d.get("reason","") for d in diags]
disagree_diags = [r for r in reasons if "disagreement" in r.lower()]
assert len(disagree_diags) >= 1, \
    f"expected guard-position vs param-name disagreement diagnostic; got: {reasons}"
refusals = [r for r in reasons if "not in learned vocabulary" in r.lower() or "unlearned" in r.lower()]
assert len(refusals) >= 1, \
    f"expected unlearned refusal for the call site; got: {reasons}"
print(f"PASS: CROSS-CHECK — guard/param-name disagreement → UNLEARNED + report.")
print(f"      Report: {disagree_diags[0][:120]}")
print(f"      Call-site refusal: {refusals[0][:90]}")
PY

# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 24: TRUTH classification from the guard — assertTrue(p(2)) lifts"
echo "────────────────────────────────────────────────────────────────"
# assertTrue can ONLY be in the vocab via its guard `if (!condition) failNotTrue(...)`
# in vendored AssertTrue.java — every name rule is deleted (test 20).
RESULT24="$(run_lift "$FIXTURES" "TruthLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT24" <<'PY'
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
assert len(ir) == 1, \
    f"truth: expected 1 contract from assertTrue(p(2)), got {len(ir)}: {json.dumps(ir,indent=2)}\ndiags={json.dumps(diags,indent=2)}"
print(f"PASS: TRUTH via guard — assertTrue(p(2)) lifted: {ir[0]['name']}")
print(f"      assertTrue entered the vocab ONLY through its `!condition` throw-guard.")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 25: IDENTITY-GUARD discrimination — reference == is NOT value equality"
echo "────────────────────────────────────────────────────────────────"
# fixtures/identity-guard/framework/IdentityAssert.java: assertNotSame guards on
# `expected == actual` over OBJECTS (reference identity — two .equals() values
# can be distinct refs) → must be UNLEARNED, never lifted as value-≠. The SAME
# guard shape over primitive ints (assertEqualsInt) must still classify and lift.
# Every Java developer knows == vs .equals; so must the lifter.
RESULT25="$(run_lift "$FIXTURES/identity-guard" "IdentityGuard.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT25" <<'PY'
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
# Exactly ONE contract: the primitive overload. The identity assert refuses.
assert len(ir) == 1, \
    f"IDENTITY FALSEPASS or over-refusal: expected exactly 1 contract (primitive), got {len(ir)}: {json.dumps(ir,indent=2)}\ndiags={json.dumps(diags,indent=2)}"
assert "callresult_g" in ir[0]["name"], f"the lifted contract must be the primitive one: {ir[0]['name']}"
reasons = [d.get("reason","") for d in diags]
ident_diags = [r for r in reasons if "assertNotSame" in r or "unlearned" in r.lower() or "vocabulary" in r.lower()]
assert len(ident_diags) >= 1, \
    f"expected a named refusal for the reference-identity assert; got: {reasons}"
print(f"PASS: IDENTITY-GUARD — reference == refused ({ident_diags[0][:80]}...); primitive != lifted ({ir[0]['name']})")
PY

# ──────────────────────────────────────────────────────────────
# G1 tests (26-30): universe-walk — the implementation body, walked through
# its own grammar, defines the valid universe of the output.
# Fixture workspaces: fixtures/universe{,-mutable,-escape}/ each carry their
# own .sugar/config.toml with vendor_source_dirs.
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 26: universe row — walked table + pad for the false-branch chain"
echo "────────────────────────────────────────────────────────────────"
RESULT26="$(run_lift "$FIXTURES/universe" "UniverseLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT26" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]

def atoms(c):
    return c["inv"]["operands"]

# encodeUpper("x") callsite: equality + universe under the SAME #euf# name.
name_x = "encodeUpper#euf#c:callresult_encodeUpper_a1(s:x)::assertion"
eq_x = [c for c in ir if c["name"] == name_x and atoms(c)[0]["name"] == "="]
un_x = [c for c in ir if c["name"] == name_x and atoms(c)[0]["name"] == "str.chars-in-set"]
assert len(eq_x) == 1, f"expected 1 equality contract named {name_x}: {[c['name'] for c in ir]}\ndiags={json.dumps(diags,indent=2)}"
assert len(un_x) == 1, f"expected 1 universe contract named {name_x}: {[c['name'] for c in ir]}\ndiags={json.dumps(diags,indent=2)}"

# The universe set: UPPER_TABLE chars + the pad char the vendor's own
# `if (outTable == UPPER_TABLE)` guard attributes — sorted+deduped "=ABC".
atom = atoms(un_x[0])[0]
subject, charset = atom["args"]
assert subject["kind"] == "ctor" and subject["name"] == "call:encodeUpper", f"bad subject: {subject}"
assert subject["args"][0] == {"kind":"const","value":"x","sort":{"kind":"primitive","name":"String"}}, \
    f"subject arg must be the String literal callsite key: {subject['args']}"
assert charset == {"kind":"const","value":"=ABC","sort":{"kind":"primitive","name":"String"}}, \
    f"walked charset wrong (want '=ABC' = UPPER_TABLE + pad, sorted): {charset}"

# The equality contract is String-sorted on the same subject.
eq_atom = atoms(eq_x[0])[0]
assert eq_atom["args"][0] == subject, "equality and universe must share the SAME subject term"
assert eq_atom["args"][1] == {"kind":"const","value":"AB=","sort":{"kind":"primitive","name":"String"}}, \
    f"sworn equality const wrong: {eq_atom['args'][1]}"
print("PASS: universe row — set '=ABC' walked (table literals + ==-guard pad), same #euf# name as the sworn equality")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 27: table discrimination — true-branch chain gets the OTHER table, no pad"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT26" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
name_y = "encodeLower#euf#c:callresult_encodeLower_a1(s:y)::assertion"
un_y = [c for c in ir if c["name"] == name_y and c["inv"]["operands"][0]["name"] == "str.chars-in-set"]
assert len(un_y) == 1, f"expected 1 universe contract for encodeLower: {[c['name'] for c in ir]}"
charset = un_y[0]["inv"]["operands"][0]["args"][1]
# LOWER_TABLE only: the vendor's pad guard names UPPER_TABLE, so the lower
# universe must NOT contain '=' — pad attribution is walked, not assumed.
assert charset == {"kind":"const","value":"abc","sort":{"kind":"primitive","name":"String"}}, \
    f"walked charset wrong (want 'abc', NO pad): {charset}"
print("PASS: table discrimination — encodeLower universe is 'abc' (true branch, pad NOT attributed)")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 28: mutable-table discrimination — non-static-final table refused by name"
echo "────────────────────────────────────────────────────────────────"
RESULT28="$(run_lift "$FIXTURES/universe-mutable" "MutableLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT28" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]
universe = [c for c in ir if c["inv"]["operands"][0]["name"] == "str.chars-in-set"]
assert len(universe) == 0, f"MUTABLE FALSEPASS: universe row emitted over a non-final table: {json.dumps(universe,indent=2)}"
# The sworn equality itself still lifts (it is the consumer's claim, not the universe).
eqs = [c for c in ir if c["inv"]["operands"][0]["name"] == "="]
assert len(eqs) == 1, f"expected the equality contract to still lift: {[c['name'] for c in ir]}"
reasons = [d.get("reason","") for d in diags]
named = [r for r in reasons if "mutable table is no axiom" in r]
assert named, f"expected the mutable-table refusal by name, got: {reasons}"
print(f"PASS: mutable-table discrimination — no universe row; named refusal: {named[0][:90]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 29: chain-escape discrimination — delegation leaves vendored source"
echo "────────────────────────────────────────────────────────────────"
RESULT29="$(run_lift "$FIXTURES/universe-escape" "EscapeLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT29" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]
universe = [c for c in ir if c["inv"]["operands"][0]["name"] == "str.chars-in-set"]
assert len(universe) == 0, f"ESCAPE FALSEPASS: universe row emitted across a chain escape: {json.dumps(universe,indent=2)}"
eqs = [c for c in ir if c["inv"]["operands"][0]["name"] == "="]
assert len(eqs) == 1, f"expected the equality contract to still lift: {[c['name'] for c in ir]}"
reasons = [d.get("reason","") for d in diags]
named = [r for r in reasons if "chain escapes vendored source" in r]
assert named, f"expected the chain-escape refusal by name, got: {reasons}"
print(f"PASS: chain-escape discrimination — no universe row; named refusal: {named[0][:90]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 30: string-literal args lift (getBytes + getBytesUtf8 shapes); non-literal still refused"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT26" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]
names = [c["name"] for c in ir]
# getBytesUtf8("z") shape lifts with arg-sig s:z (and its universe twin).
name_z = "encodeUpper#euf#c:callresult_encodeUpper_a1(s:z)::assertion"
assert names.count(name_z) == 2, f"getBytesUtf8 shape must lift equality+universe under {name_z}: {names}"
# "x".getBytes() shape lifted in TEST 26 (s:x). Total: 3 callsites x 2 rows.
assert len(ir) == 6, f"expected 6 contracts (3 string callsites x equality+universe), got {len(ir)}: {names}"
# The non-literal arg (variable `data`) is REFUSED by name — no s:w contract.
assert not any("s:w" in n for n in names), f"non-literal arg was lifted (falsePass): {names}"
reasons = [d.get("reason","") for d in diags]
named = [r for r in reasons if "is not an int literal or getBytesUtf8/getBytes" in r]
assert named, f"expected the non-literal-arg refusal by name, got: {reasons}"
print(f"PASS: string-literal args lift via both byte-bridge shapes; non-literal refused: {named[0][:80]}")
PY

# H1 tests (31+): hardening rollup — discrimination fixtures for each fix.
# Each H1 test has a twin fixture that BREAKS the invariant on purpose, so
# a regression (reverting the fix) turns the test red, not green.
# ──────────────────────────────────────────────────────────────
echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 31: H1 [A1] cross-class ambiguity → UNLEARNED (not first-match)"
echo "────────────────────────────────────────────────────────────────"
RESULT31="$(run_lift "$FIXTURES/cross-class-ambiguity" "CrossClassAmbiguity.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT31" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
# Must produce 0 contracts — checkEq is UNLEARNED due to ambiguous delegation.
assert len(ir) == 0, (
    f"FALSEPASS: cross-class ambiguity produced {len(ir)} contract(s) "
    f"(first-match silently chose a class): {[c['name'] for c in ir]}"
)
# Must emit a named refusal citing ambiguous delegation.
reasons = [d.get("reason", "") for d in diags]
named = [r for r in reasons if "ambiguous" in r.lower()]
assert named, (
    f"expected a named refusal citing 'ambiguous delegation target', got: {reasons}"
)
# The refusal must NOT say "equality" or "inequality" — wrong classification guard.
bad = [r for r in reasons if "equality" in r.lower() or "inequality" in r.lower()]
assert not bad, f"refusal incorrectly reports equality/inequality: {bad}"
print(f"PASS: cross-class ambiguity → 0 contracts, named UNLEARNED refusal: {named[0][:90]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 32a: H1 [A2] wildcard static import expands to all vendored vocab names"
echo "────────────────────────────────────────────────────────────────"
RESULT32A="$(run_lift "$FIXTURES" "WildcardImportLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT32A" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
# Wildcard import must expand assertEquals so it lifts exactly as named-import twin.
assert len(ir) == 1, (
    f"FALSEPASS: wildcard import produced {len(ir)} contract(s) "
    f"(expected 1 for assertEquals/equality): {[c['name'] for c in ir]}\ndiags={diags}"
)
c = ir[0]
inv = c["inv"]
atomic = inv["operands"][0]
assert atomic["kind"] == "atomic" and atomic["name"] == "=", (
    f"expected '=' relation (equality), got: {atomic['name']}"
)
print(f"PASS: wildcard static import → assertEquals expanded, 1 equality contract lifted: {c['name'][:80]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 32b: H1 [A2] non-framework wildcard import → not bound, silent skip"
echo "────────────────────────────────────────────────────────────────"
RESULT32B="$(run_lift "$FIXTURES" "WildcardUnvendoredRefusal.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT32B" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
# Non-framework wildcard (com.example.*) not processed by import scanner.
# The bare assertEquals is not framework-bound → silent skip: 0 contracts, 0 diagnostics.
assert len(ir) == 0, (
    f"FALSEPASS: non-framework wildcard produced {len(ir)} contract(s): {[c['name'] for c in ir]}"
)
# No diagnostic: the call is silently not-framework-bound (not an error, just not ours).
bad_diags = [d for d in diags if "assertEquals" in d.get("reason","").lower()]
assert not bad_diags, (
    f"unexpected diagnostic about assertEquals from non-framework wildcard: {bad_diags}"
)
print(f"PASS: non-framework wildcard import → assertEquals not bound, silent skip (0 contracts)")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 33: H1 [A3] user-scope impostor assertEquals (no static import) → silent skip"
echo "────────────────────────────────────────────────────────────────"
RESULT33="$(run_lift "$FIXTURES" "UserScopeImpostor.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT33" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
# User-scope impostor: 0 contracts (silent skip — not framework-bound).
# Must NOT produce an equality lift (falsePass) or a false refusal.
assert len(ir) == 0, (
    f"FALSEPASS: user-scope assertEquals was lifted as a framework assertion: "
    f"{[c['name'] for c in ir]}"
)
# The import-edge guard silently skips — no diagnostic expected.
bad_lift = [d for d in diags if "equality" in d.get("reason","").lower()
            and "impostor" not in d.get("reason","").lower()]
assert not bad_lift, f"unexpected equality-related diagnostic: {bad_lift}"
print(f"PASS: user-scope impostor assertEquals silently skipped (0 contracts, import-edge guard)")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 34: H1 [C8] TestNG assertNotEquals 2-arg form → INEQUALITY (not APPROX)"
echo "────────────────────────────────────────────────────────────────"
RESULT34="$(run_lift "$FIXTURES" "TestNGAssertNotEquals.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT34" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
# 2-arg assertNotEquals must lift as INEQUALITY, NOT as "approx" refusal.
approx = [d for d in diags if "approximate" in d.get("reason","").lower()]
assert not approx, (
    f"REGRESSION: 2-arg assertNotEquals classified as APPROX (delta overload wins): {approx}"
)
assert len(ir) == 1, (
    f"expected 1 INEQUALITY contract from assertNotEquals(g(2), 3), got {len(ir)}: "
    f"{[c['name'] for c in ir]}\ndiags={[d.get('reason','') for d in diags]}"
)
c = ir[0]
inv = c["inv"]
atomic = inv["operands"][0]
assert atomic["name"] in ("!=", "≠"), (
    f"expected inequality relation ('!=' or '≠'), got: {atomic['name']}"
)
print(f"PASS: TestNG assertNotEquals 2-arg → 1 inequality contract (not approx): {c['name'][:80]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 35: H1 [B6] lineLength=0 sound; lineLength=76 refused (chunking guard)"
echo "────────────────────────────────────────────────────────────────"
RESULT35="$(run_lift "$FIXTURES/universe-chunked" "ChunkedLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT35" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
names = [c["name"] for c in ir]
# encodeNoChunk (lineLength=0) must register and produce a universe contract.
no_chunk_universe = [c for c in ir
    if "encodeNoChunk" in c["name"]
    and c["inv"]["operands"][0]["name"] == "str.chars-in-set"]
assert no_chunk_universe, (
    f"MISSING: encodeNoChunk universe row not found. Contracts: {names}\ndiags={[d.get('reason','') for d in diags]}"
)
# encodeChunked (lineLength=76) must be REFUSED by the universe walker.
# No universe row for encodeChunked must appear.
chunked_universe = [c for c in ir
    if "encodeChunked" in c["name"]
    and c["inv"]["operands"][0]["name"] == "str.chars-in-set"]
assert not chunked_universe, (
    f"FALSEPASS: encodeChunked universe row was registered (lineLength=76 not detected): "
    f"{[c['name'] for c in chunked_universe]}"
)
# The universe walker must emit a named refusal for encodeChunked.
reasons = [d.get("reason", "") for d in diags]
chunked_refusal = [r for r in reasons if "encodeChunked" in r or "chunking" in r.lower() or "lineLength" in r]
assert chunked_refusal, (
    f"expected a named refusal for encodeChunked (lineLength=76), got: {reasons}"
)
print(f"PASS: lineLength=0 → encodeNoChunk universe registered; "
      f"lineLength=76 → encodeChunked refused: {chunked_refusal[0][:80]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 36: G2 abs truth — equality + int32.eq-bv-expr row under SAME #euf# name"
echo "────────────────────────────────────────────────────────────────"
RESULT36="$(run_lift "$FIXTURES/numeric-universe" "NumericUniverseLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT36" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]

# testAbsTruth: assertEquals(-2147483648, abs(-2147483648))
# int arg → argSig = "i:-2147483648"
name = "abs#euf#c:callresult_abs_a1(i:-2147483648)::assertion"
eq_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "="]
bv_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "int32.eq-bv-expr"]
assert len(eq_rows) == 1, (
    f"expected 1 equality row for abs(-2147483648), got {len(eq_rows)}. "
    f"All contracts: {[c['name'] for c in ir]}\ndiags={[d.get('reason','') for d in diags]}"
)
assert len(bv_rows) == 1, (
    f"expected 1 int32.eq-bv-expr row for abs(-2147483648), got {len(bv_rows)}. "
    f"All contracts: {[c['name'] for c in ir]}\ndiags={[d.get('reason','') for d in diags]}"
)
print(f"PASS: G2 abs truth — equality + int32.eq-bv-expr row, both named: {name[:80]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 37: G2 bv-expr structure — bv32.ite(bv32.slt(a,0), bv32.neg(a), a)"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT36" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]

name = "abs#euf#c:callresult_abs_a1(i:-2147483648)::assertion"
bv_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "int32.eq-bv-expr"]
assert len(bv_rows) == 1, f"no int32.eq-bv-expr row: {[c['name'] for c in ir]}"

atom = bv_rows[0]["inv"]["operands"][0]
assert atom["name"] == "int32.eq-bv-expr", f"atom name wrong: {atom['name']}"
assert len(atom["args"]) == 2, f"expected 2 args, got {len(atom['args'])}"

# args[0]: call:abs ctor
subj = atom["args"][0]
assert subj["kind"] == "ctor" and subj["name"] == "call:abs", f"subject wrong: {subj}"
assert subj["args"][0] == {"kind":"const","value":-2147483648,"sort":{"kind":"primitive","name":"Int"}}, \
    f"subject arg wrong: {subj['args']}"

# args[1]: the BV expression tree — bv32.ite
bv = atom["args"][1]
assert bv["kind"] == "ctor" and bv["name"] == "bv32.ite", f"bv root wrong: {bv}"
slt, neg, var = bv["args"]
assert slt["kind"] == "ctor" and slt["name"] == "bv32.slt", f"slt wrong: {slt}"
assert neg["kind"] == "ctor" and neg["name"] == "bv32.neg", f"neg wrong: {neg}"
assert var["kind"] == "var" and var["name"] == "a", f"false-branch var wrong: {var}"
# slt args: var a, const 0
slt_lhs, slt_rhs = slt["args"]
assert slt_lhs["kind"] == "var" and slt_lhs["name"] == "a", f"slt lhs wrong: {slt_lhs}"
assert slt_rhs["kind"] == "const" and slt_rhs["value"] == 0, f"slt rhs wrong: {slt_rhs}"
# neg arg: var a
neg_arg = neg["args"][0]
assert neg_arg["kind"] == "var" and neg_arg["name"] == "a", f"neg arg wrong: {neg_arg}"

print("PASS: G2 bv-expr structure — bv32.ite(bv32.slt(a,0), bv32.neg(a), a) confirmed")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 38: G2 positive/negative abs cases also emit universe rows"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT36" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]

# testAbsPositive: assertEquals(5, abs(5))  → argSig i:5
# testAbsNegative: assertEquals(5, abs(-5)) → argSig i:-5
for arg, desc in [("5", "abs(5)"), ("-5", "abs(-5)")]:
    name = f"abs#euf#c:callresult_abs_a1(i:{arg})::assertion"
    eq_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "="]
    bv_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "int32.eq-bv-expr"]
    assert len(eq_rows) == 1, f"missing equality row for {desc}: {[c['name'] for c in ir]}"
    assert len(bv_rows) == 1, (
        f"missing int32.eq-bv-expr row for {desc}: {[c['name'] for c in ir]}\n"
        f"diags={[d.get('reason','') for d in diags]}"
    )
print("PASS: G2 positive/negative abs cases — both emit equality + universe row pairs")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 39: G2 bad-shape discrimination — non-ternary body refused by name; equality still lifts"
echo "────────────────────────────────────────────────────────────────"
RESULT39="$(run_lift "$FIXTURES/numeric-universe-bad-shape" "NumericBadShapeLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT39" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]

# The equality contract must still lift
eq_rows = [c for c in ir if c["inv"]["operands"][0]["name"] == "="]
assert len(eq_rows) >= 1, (
    f"equality contract missing even though clamp(0)==0 should lift: "
    f"{[c['name'] for c in ir]}\ndiags={[d.get('reason','') for d in diags]}"
)
# No int32.eq-bv-expr row must be emitted
bv_rows = [c for c in ir if c["inv"]["operands"][0]["name"] == "int32.eq-bv-expr"]
assert len(bv_rows) == 0, (
    f"FALSEPASS: int32.eq-bv-expr row emitted for unsupported shape: {json.dumps(bv_rows,indent=2)}"
)
# The walker must have emitted a named refusal for the unsupported shape
reasons = [d.get("reason", "") for d in diags]
refused = [r for r in reasons if "numeric universe walk refused" in r or "shape" in r.lower() or "not supported" in r.lower() or "not a ternary" in r.lower()]
assert refused, (
    f"expected a named refusal for unsupported shape, got no matching diagnostic: {reasons}"
)
print(f"PASS: G2 bad-shape discrimination — equality lifts, bv-expr refused: {refused[0][:80]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 40: G2 no-vendor-dir — equality lifts, no universe row (numeric registry empty)"
echo "────────────────────────────────────────────────────────────────"
# Use the no-vocab fixture (has no vendor_source_dirs) — assertEquals is unlearned there.
# Instead use the fixtures root (which has junit5 vendor but no numeric vendor_source_dirs).
# The base FIXTURES/.sugar/config.toml has no vendor_source_dirs key → numericRegistry = EMPTY.
python3 - "$RESULT1" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]

# Equality contracts must be present (this fixture always lifts them)
eq_rows = [c for c in ir if c["inv"]["operands"][0]["name"] == "="]
assert len(eq_rows) >= 1, f"expected equality contracts: {[c['name'] for c in ir]}"
# No int32.eq-bv-expr rows (no vendor_source_dirs → no numeric universe)
bv_rows = [c for c in ir if c["inv"]["operands"][0]["name"] == "int32.eq-bv-expr"]
assert len(bv_rows) == 0, (
    f"FALSEPASS: int32.eq-bv-expr row emitted without a vendor_source_dirs config: "
    f"{json.dumps(bv_rows,indent=2)}"
)
print("PASS: G2 no-vendor-dir — equality lifts; numeric registry empty, no int32.eq-bv-expr row")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 41: G2b assertTrue(g(7) < 10) → <(call:g(7), 10) with #euf# name"
echo "────────────────────────────────────────────────────────────────"
RESULT41="$(run_lift "$FIXTURES" "ComparisonBoundLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT41" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]

# assertTrue(g(7) < 10)
name = "g#euf#c:callresult_g_a1(i:7)::assertion"
lt_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "<"]
assert len(lt_rows) == 1, (
    f"expected 1 '<' contract for g(7), got {len(lt_rows)}. "
    f"ir={[c['name']+':'+c['inv']['operands'][0]['name'] for c in ir]}"
)
atom = lt_rows[0]["inv"]["operands"][0]
assert atom["args"][0] == {"kind":"ctor","name":"call:g","args":[{"kind":"const","value":7,"sort":{"kind":"primitive","name":"Int"}}]}, \
    f"call arg wrong: {atom['args'][0]}"
assert atom["args"][1] == {"kind":"const","value":10,"sort":{"kind":"primitive","name":"Int"}}, \
    f"lit arg wrong: {atom['args'][1]}"
print(f"PASS: G2b assertTrue(g(7) < 10) → <(call:g(7),10) name={name}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 42: G2b assertTrue(5 > g(2)) → <(call:g(2), 5) [lit-left mirror]"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT41" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]

# assertTrue(5 > g(2)) — lit on left, mirrored: g(2) < 5
# testLessThanLitLeft emits this; testAssertFalse also emits for g(2) >= 5
# Check the < row exists (from testLessThanLitLeft)
name = "g#euf#c:callresult_g_a1(i:2)::assertion"
lt_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == "<"]
gte_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == ">="]
assert len(lt_rows) == 1, f"expected 1 '<' contract for g(2) (mirrored 5>g(2)): {[(c['name'],c['inv']['operands'][0]['name']) for c in ir]}"
assert len(gte_rows) == 1, f"expected 1 '>=' contract for g(2) (assertFalse(g(2)<5)): {[(c['name'],c['inv']['operands'][0]['name']) for c in ir]}"
lt_atom = lt_rows[0]["inv"]["operands"][0]
assert lt_atom["args"][1] == {"kind":"const","value":5,"sort":{"kind":"primitive","name":"Int"}}, \
    f"lit wrong for mirrored: {lt_atom['args'][1]}"
print(f"PASS: G2b assertTrue(5>g(2)) → <(call:g(2),5); assertFalse(g(2)<5) → >=(call:g(2),5)")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 43: G2b assertFalse(g(2) < 5) → >=(call:g(2), 5) [negated predicate]"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT41" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]

name = "g#euf#c:callresult_g_a1(i:2)::assertion"
gte_rows = [c for c in ir if c["name"] == name and c["inv"]["operands"][0]["name"] == ">="]
assert len(gte_rows) == 1, f"expected 1 '>=' contract for assertFalse(g(2)<5): {[(c['name'],c['inv']['operands'][0]['name']) for c in ir]}"
gte_atom = gte_rows[0]["inv"]["operands"][0]
assert gte_atom["args"][0]["name"] == "call:g", f"call wrong: {gte_atom['args'][0]}"
assert gte_atom["args"][1] == {"kind":"const","value":5,"sort":{"kind":"primitive","name":"Int"}}, \
    f"lit wrong: {gte_atom['args'][1]}"
print(f"PASS: G2b assertFalse(g(2)<5) → >=(call:g(2),5) confirmed")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 44: G2b non-literal bound refused by name; both-calls refused by name"
echo "────────────────────────────────────────────────────────────────"
RESULT44="$(run_lift "$FIXTURES" "ComparisonBoundDiscrimination.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT44" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"
ir = result["ir"]
diags = result["diagnostics"]
reasons = [d.get("reason", "") for d in diags]

# No contracts should be emitted
assert len(ir) == 0, f"expected no contracts, got {[c['name'] for c in ir]}"

# non-literal bound refused
non_lit = [r for r in reasons if "non-literal bound" in r or "not an int literal" in r.lower()]
assert non_lit, f"expected non-literal-bound refusal, got reasons: {reasons}"

# both-calls refused
both_calls = [r for r in reasons if "both operands are calls" in r]
assert both_calls, f"expected both-operands-are-calls refusal, got reasons: {reasons}"

print(f"PASS: G2b discrimination — non-literal bound refused: {non_lit[0][:80]}")
print(f"PASS: G2b discrimination — both-calls refused: {both_calls[0][:80]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 45: G2b #euf# name is IDENTICAL to assertEquals name for same callsite"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT41" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]

# The contract name for assertTrue(g(7) < 10) must be IDENTICAL to what
# assertEquals(expected, g(7)) would produce — same #euf# schema.
expected_name = "g#euf#c:callresult_g_a1(i:7)::assertion"
names = [c["name"] for c in ir]
assert expected_name in names, (
    f"assertTrue(g(7)<10) must produce name '{expected_name}' for federation; "
    f"got names: {names}"
)
print(f"PASS: G2b #euf# name '{expected_name}' matches assertEquals schema — federation is live")
PY


# ──────────────────────────────────────────────────────────────
# TEST SUITE P5c — call-binding lift (SSA substitution + instance-method location-keyed)
# Mirrors Python PATTERN 5 / _apply_value_scope_binding + _call_origin_from_expr.
#
# 46. SSA static: `int r = compute(7); assertEquals(14, r)` → same #euf# name as inline form
# 47. SSA static byte-identity: SSA and inline forms produce byte-identical contract names
# 48. Instance-method: `Codec c = new Codec(); assertEquals(0, c.getPolicy())` → location-keyed
# 49. Two-test no-collision: two test methods with different constructions → different location bases
# 50. Reassigned local refused by name
# ──────────────────────────────────────────────────────────────

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 46: P5c SSA static — effectively-final local substituted to #euf# call"
echo "────────────────────────────────────────────────────────────────"
RESULT46="$(run_lift "$FIXTURES" "P5cSsaStaticLiftInt.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT46" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]

# Both testViaLocal and testInline must produce a contract
assert len(ir) == 2, f"expected 2 contracts (one per test), got {len(ir)}: {[c['name'] for c in ir]}"

# Both must use #euf# (static call federates)
for c in ir:
    assert "#euf#" in c["name"], f"expected #euf# in name (static call), got: {c['name']}"

print(f"PASS: P5c SSA static — {len(ir)} contracts, both #euf#-federated: {ir[0]['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 47: P5c SSA byte-identity — local form and inline form produce SAME contract name"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT46" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]

names = [c["name"] for c in ir]
expected_name = "compute#euf#c:callresult_compute_a1(i:7)::assertion"
assert names.count(expected_name) == 2, (
    f"SSA and inline forms must produce byte-identical contract names; "
    f"expected 2x '{expected_name}', got: {names}"
)
print(f"PASS: P5c byte-identity — SSA form and inline form produce same name: {expected_name}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 48: P5c instance-method — local receiver → location-keyed (NOT #euf#)"
echo "────────────────────────────────────────────────────────────────"
RESULT48="$(run_lift "$FIXTURES" "P5cInstanceMethodLift.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT48" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]

# Must produce contracts (the instance-method calls are liftable)
assert len(ir) >= 1, f"expected contracts from instance-method lift, got 0 (diags: {diags})"

# MUST NOT use #euf# — instance-method calls are location-keyed
for c in ir:
    assert "#euf#" not in c["name"], (
        f"instance-method call must NOT be #euf#-federated; got: {c['name']}"
    )

# Must contain the scope (file::class::method) in the name — location-keyed
for c in ir:
    assert "P5cInstanceMethodLift" in c["name"], (
        f"location-keyed name must contain the class/method scope; got: {c['name']}"
    )

print(f"PASS: P5c instance-method — {len(ir)} location-keyed contracts (no #euf#): {ir[0]['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 49: P5c no-collision — two test methods with same method name produce different names"
echo "────────────────────────────────────────────────────────────────"
python3 - "$RESULT48" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]

# Both test methods (testCodecPolicyDefault, testCodecPolicyStrict) must produce contracts
assert len(ir) == 2, f"expected 2 contracts (one per test method), got {len(ir)}: {[c['name'] for c in ir]}"

# The two names must be DIFFERENT (different test method scope)
assert ir[0]["name"] != ir[1]["name"], (
    f"two different test methods must produce different location-keyed names; "
    f"both produced: {ir[0]['name']}"
)

# Each name must encode the respective test method
names = [c["name"] for c in ir]
assert any("testCodecPolicyDefault" in n for n in names), f"first test name not in: {names}"
assert any("testCodecPolicyStrict" in n for n in names), f"second test name not in: {names}"

print(f"PASS: P5c no-collision — two tests, two distinct names:")
print(f"  {ir[0]['name']}")
print(f"  {ir[1]['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 50: P5c reassigned local refused by name (not a stable SSA alias)"
echo "────────────────────────────────────────────────────────────────"
RESULT50="$(run_lift "$FIXTURES" "P5cReassignedRefusal.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT50" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None
ir = result["ir"]
diags = result["diagnostics"]

assert len(ir) == 0, f"reassigned local must produce 0 contracts, got {len(ir)}: {[c['name'] for c in ir]}"
assert len(diags) >= 1, f"expected a named refusal for reassigned local, got: {diags}"
reasons = [d.get("reason", "") for d in diags]
ssa_diags = [r for r in reasons if "reassigned" in r.lower() or "stable" in r.lower() or "SSA" in r]
assert ssa_diags, f"expected a refusal naming 'reassigned' or 'stable SSA alias', got: {reasons}"
print(f"PASS: P5c reassigned local refused by name: {ssa_diags[0]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST SUITE G3 — instance-universe: construction-semantics walk through \`this\`"
echo "Pins call:get(x) == ctorValue when getter is a pure final-field return."
echo "────────────────────────────────────────────────────────────────"

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 51: G3 positive — pure final-field getter emits TWO operands (ctor fact + test claim)"
echo "────────────────────────────────────────────────────────────────"
RESULT51="$(run_lift "$FIXTURES" "G3BoxPositiveTest.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT51" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None

ir = result["ir"]
assert len(ir) == 1, f"expected 1 contract, got {len(ir)}: {[c['name'] for c in ir]}"
c = ir[0]

# Must be location-keyed (contains ::assertion, not #euf#)
assert "::assertion" in c["name"], f"name should be location-keyed: {c['name']}"
assert "#euf#" not in c["name"], f"must not be #euf#-federated: {c['name']}"

inv = c["inv"]
assert inv["kind"] == "and", f"inv.kind: {inv['kind']}"
ops = inv["operands"]
assert len(ops) == 2, (
    f"G3 positive: expected 2 operands (ctor fact + test claim), got {len(ops)}: {ops}")

# Both operands must be atomic '='
for i, op in enumerate(ops):
    assert op["kind"] == "atomic" and op["name"] == "=", f"operand[{i}]: {op}"

# Both must have the SAME ctorJson (byte-identical call:get(x) term)
ctor0 = ops[0]["args"][0]
ctor1 = ops[1]["args"][0]
assert ctor0 == ctor1, f"ctorJson not byte-identical: {ctor0} vs {ctor1}"

# operand[0] is the construction fact (value 5 from new G3Box(5))
const0 = ops[0]["args"][1]
assert const0["kind"] == "const" and const0["value"] == 5, (
    f"construction fact should be const(5,Int), got: {const0}")

# operand[1] is the test's claim (assertEquals(5, x.get()))
const1 = ops[1]["args"][1]
assert const1["kind"] == "const" and const1["value"] == 5, (
    f"test claim should be const(5,Int), got: {const1}")

print(f"PASS: G3 positive — 2 operands, both const(5,Int), byte-identical ctorJson: {ctor0['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 52: G3 discrimination — non-final field refused, contract has ONE operand"
echo "────────────────────────────────────────────────────────────────"
RESULT52="$(run_lift "$FIXTURES" "G3NonFinalDiscriminationTest.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT52" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None

ir = result["ir"]
diags = result["diagnostics"]

# Contract must still lift (the test assertion itself is valid)
assert len(ir) == 1, f"expected 1 contract, got {len(ir)}: {[c['name'] for c in ir]}"
c = ir[0]
inv = c["inv"]
ops = inv["operands"]

# Must have exactly ONE operand (construction refused — non-final field)
assert len(ops) == 1, (
    f"G3 non-final discrimination: expected 1 operand (no ctor pin), got {len(ops)}: {ops}")

# Must have a named diagnostic mentioning the refusal
reasons = [d.get("reason", "") for d in diags]
refusal_diags = [r for r in reasons if "not final" in r or "construction not pinned" in r]
assert refusal_diags, (
    f"expected a diagnostic naming 'not final' or 'construction not pinned', got: {reasons}")

print(f"PASS: G3 non-final discrimination — 1 operand (no pin), refusal: {refusal_diags[0]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 53: G3 discrimination — getter with computation refused, contract has ONE operand"
echo "────────────────────────────────────────────────────────────────"
RESULT53="$(run_lift "$FIXTURES" "G3ComputationDiscriminationTest.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT53" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None

ir = result["ir"]

# Contract must still lift (the test assertion itself is valid)
assert len(ir) == 1, f"expected 1 contract, got {len(ir)}: {[c['name'] for c in ir]}"
c = ir[0]
inv = c["inv"]
ops = inv["operands"]

# Must have exactly ONE operand (construction refused — getter is not a pure field read)
assert len(ops) == 1, (
    f"G3 computation discrimination: expected 1 operand (no ctor pin), got {len(ops)}: {ops}")

print(f"PASS: G3 computation discrimination — 1 operand (computation getter refused, no pin)")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST SUITE Voltron — mutually-recursive construction-semantics resolver"
echo "Receiver w.unwrap().get() is itself a call; two-layer walk Box→Wrapper→int."
echo "────────────────────────────────────────────────────────────────"

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 54: Voltron positive — two-layer chain emits TWO operands, pinned const=5"
echo "────────────────────────────────────────────────────────────────"
RESULT54="$(run_lift "$FIXTURES" "VoltronPositiveTest.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT54" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"

ir = result["ir"]
diags = result["diagnostics"]

# Must have exactly one contract.
assert len(ir) == 1, f"expected 1 contract, got {len(ir)}: {ir}"

c = ir[0]

# Must be location-keyed (contains ::assertion, not #euf#).
assert "::assertion" in c["name"], f"expected location-keyed name, got: {c['name']}"
assert "#euf#" not in c["name"], f"unexpected federation in name: {c['name']}"

inv = c["inv"]
ops = inv["operands"]

# Must have exactly TWO operands.
assert len(ops) == 2, f"Voltron positive: expected 2 operands (ctor pin + test claim), got {len(ops)}: {ops}"

# Both operands must be atomic '='.
for i, op in enumerate(ops):
    assert op["kind"] == "atomic" and op["name"] == "=", f"operand[{i}]: {op}"

# Both must use the SAME ctorJson (byte-identical receiver term).
ctor0 = ops[0]["args"][0]
ctor1 = ops[1]["args"][0]
assert ctor0 == ctor1, f"receiver term not byte-identical across operands: {ctor0} vs {ctor1}"

# Receiver term must be a ctor/call wrapping a var.
assert ctor0["kind"] == "ctor", f"expected kind=ctor, got: {ctor0['kind']}"
assert ctor0["args"][0]["kind"] == "var", f"expected var arg, got: {ctor0['args'][0]}"

# operand[0] is the construction fact (value 5).
const0 = ops[0]["args"][1]
assert const0["kind"] == "const" and const0["value"] == 5, (
    f"ctor pin should be const(5,Int), got: {const0}")

# operand[1] is the test's claim (assertEquals(5, ...)).
const1 = ops[1]["args"][1]
assert const1["kind"] == "const" and const1["value"] == 5, (
    f"test claim should be const(5,Int), got: {const1}")

# No diagnostics — clean resolution.
assert len(diags) == 0, f"expected no diagnostics, got: {diags}"

print(f"PASS: Voltron positive — 2 operands, both const(5,Int), byte-identical receiver term: {ctor0['name']}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 55: Voltron discrimination — non-final inner field refuses entire chain"
echo "────────────────────────────────────────────────────────────────"
RESULT55="$(run_lift "$FIXTURES" "VoltronNonFinalDiscriminationTest.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT55" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"

ir = result["ir"]
diags = result["diagnostics"]

# The whole chain must be refused — no contract emitted.
assert len(ir) == 0, (
    f"Voltron non-final discrimination: expected 0 contracts (chain refused), got {len(ir)}: {ir}")

# Must have a named diagnostic mentioning the non-final field.
reasons = [d.get("reason", "") for d in diags]
nonfinal_diags = [r for r in reasons if "not final" in r or "construction not pinned" in r]
assert nonfinal_diags, (
    f"expected a diagnostic naming 'not final' or 'construction not pinned', got: {reasons}")

print(f"PASS: Voltron non-final discrimination — whole chain refused, named diagnostic: {nonfinal_diags[0]}")
PY

echo
echo "────────────────────────────────────────────────────────────────"
echo "TEST 56: Voltron discrimination — computed inner getter refuses entire chain"
echo "────────────────────────────────────────────────────────────────"
RESULT56="$(run_lift "$FIXTURES" "VoltronComputationDiscriminationTest.java" | eval "$JAVA_CMD" 2>/dev/null)"
python3 - "$RESULT56" <<'PY'
import sys, json
lines = sys.argv[1].strip().split('\n')
result = None
for line in lines:
    if not line.strip(): continue
    obj = json.loads(line)
    if obj.get("id") == 2:
        result = obj["result"]
        break
assert result is not None, "no lift response"

ir = result["ir"]
diags = result["diagnostics"]

# The whole chain must be refused — no contract emitted.
assert len(ir) == 0, (
    f"Voltron computation discrimination: expected 0 contracts (chain refused), got {len(ir)}: {ir}")

# Must have a named diagnostic (chain refusal).
reasons = [d.get("reason", "") for d in diags]
chain_diags = [r for r in reasons if "voltron" in r.lower() or "chain" in r.lower() or "refused" in r.lower()]
assert chain_diags, (
    f"expected a diagnostic naming chain refusal, got: {reasons}")

print(f"PASS: Voltron computation discrimination — whole chain refused, named diagnostic: {chain_diags[0]}")
PY

echo
echo "== all 56 tests PASS (12 P1-P3 + 7 P4 + 6 P4.5 + 5 G1 + 5 H1 + 5 G2 + 5 G2b + 5 P5c + 3 G3 + 3 Voltron) =="
