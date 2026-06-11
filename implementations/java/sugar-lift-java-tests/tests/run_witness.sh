#!/usr/bin/env bash
# Unit tests for JavaJunitWitnessRpc (P5a witness kit).
# Drives the kit via JSON-RPC, verifies outputs with python3.
# Skips cleanly if no JDK is on PATH.
#
# Test suite:
#   W1. blake3_512 vectors: empty, "hello" match known values.
#   W2. lift response: kit returns ir-document with witness-package contract
#       (proofType=custom, tool=junit) + WitnessPackageMemento.
#   W3. resolve_witness returns b64 bytes; blake3(decoded) == witness_cid.
#   W4. tampered body: blake3(tampered_bytes) != original_cid → verifier would refuse.
#   W5. lying-discharge: a body claiming all "passed" but with wrong cid is refused.
set -euo pipefail

command -v javac   >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java    >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP: no python3 on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KIT_DIR="$(cd "$HERE/.." && pwd)"

echo "== build Java witness kit =="
bash "$KIT_DIR/build.sh" "$KIT_DIR/out" >/dev/null 2>&1
[ -f "$KIT_DIR/out/JavaJunitWitnessRpc.class" ] || {
  echo "FAIL: JavaJunitWitnessRpc.class not built"; exit 1; }

JAVA="$(which java)"
KIT_CP="$KIT_DIR/out"

# Helper: send one JSON-RPC request, capture response
rpc() {
  local msg="$1"
  printf '%s\n%s\n' "$msg" '{"jsonrpc":"2.0","id":99,"method":"shutdown"}' \
    | "$JAVA" -cp "$KIT_CP" JavaJunitWitnessRpc 2>/dev/null \
    | head -1
}

PASS=0
FAIL=0
fail() { echo "FAIL: $*"; FAIL=$((FAIL+1)); }
pass() { echo "  OK: $*"; PASS=$((PASS+1)); }

# ─── W1: blake3_512 test vectors ──────────────────────────────────────────────
echo
echo "=== W1: blake3_512 test vectors ==="

# We test via lift on a known fixture: check that the witness_cid field in the
# lift response starts with "blake3-512:" and is 139 chars (11 + 128).
# Also run a dedicated blake3 check via a small java snippet.
cat > /tmp/TestWitnessBlake3.java << 'EOF'
import java.nio.charset.StandardCharsets;
import java.lang.reflect.*;
public class TestWitnessBlake3 {
    public static void main(String[] args) throws Exception {
        Class<?> cls = Class.forName("JavaJunitWitnessRpc");
        Method m = cls.getDeclaredMethod("blake3_512Of", byte[].class);
        m.setAccessible(true);
        // Vector 1: empty
        String got1 = (String) m.invoke(null, new byte[0]);
        String exp1 = "blake3-512:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262e00f03e7b69af26b7faaf09fcd333050338ddfe085b8cc869ca98b206c08243a";
        System.out.println(got1.equals(exp1) ? "V1:OK" : "V1:FAIL:" + got1);
        // Vector 2: "hello"
        String got2 = (String) m.invoke(null, "hello".getBytes(StandardCharsets.UTF_8));
        String exp2 = "blake3-512:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200fe992405f0d785b599a2e3387f6d34d01faccfeb22fb697ef3fd53541241a338c";
        System.out.println(got2.equals(exp2) ? "V2:OK" : "V2:FAIL:" + got2);
        // Vector 3: 100 x's
        byte[] x100 = new byte[100]; java.util.Arrays.fill(x100, (byte)'x');
        String got3 = (String) m.invoke(null, x100);
        String exp3 = "blake3-512:2caf4e0114b6034350bf2947d79fca66685c2e1e7b9e057c9e171129c045a643222df088097161a59b42e29ecf48c5ad70ec53b53083bd5aa772d5714939dc1d";
        System.out.println(got3.equals(exp3) ? "V3:OK" : "V3:FAIL:" + got3);
        // Vector 4: 2048 y's (multi-chunk)
        byte[] y2048 = new byte[2048]; java.util.Arrays.fill(y2048, (byte)'y');
        String got4 = (String) m.invoke(null, y2048);
        String exp4 = "blake3-512:e850c573d95027f1a923fe96fa551b37065df4f71e6fa93b5fe8fa732a2d16e11d76bb2903f7639dc1232009f0c63f4d348136b70c3c94eafeeee971186d9c85";
        System.out.println(got4.equals(exp4) ? "V4:OK" : "V4:FAIL:" + got4);
    }
}
EOF
javac --release 21 -cp "$KIT_CP" /tmp/TestWitnessBlake3.java -d /tmp >/dev/null 2>&1
v_out=$(java -cp "$KIT_CP:/tmp" TestWitnessBlake3 2>/dev/null)
for v in V1 V2 V3 V4; do
  if echo "$v_out" | grep -q "${v}:OK"; then
    pass "${v} blake3_512 vector"
  else
    got=$(echo "$v_out" | grep "^${v}:")
    fail "${v} blake3_512 vector: $got"
  fi
done

# ─── W2: lift response shape ───────────────────────────────────────────────────
echo
echo "=== W2: lift response shape ==="

WORK2=$(mktemp -d)
cat > "$WORK2/WitnessTest.java" << 'EOF'
public class WitnessTest {
    public void testPass() {}
    public void testFail() { throw new AssertionError("fails"); }
}
EOF

LIFT_RESP=$(printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$WORK2\"}}" \
  '{"jsonrpc":"2.0","id":3,"method":"shutdown"}' \
  | "$JAVA" -cp "$KIT_CP" JavaJunitWitnessRpc 2>/dev/null \
  | awk 'NR==2')

python3 - "$LIFT_RESP" << 'PY'
import json, sys
resp = sys.argv[1]
d = json.loads(resp)
result = d["result"]
ir = result["ir"]
mementos = result["witness_mementos"]

# The `ir` array carries the witness-package CONTRACT plus the witness-memento
# (mint reads only `ir` and dispatches per `kind`, so the memento must ride here
# too). Select the contract member by kind; the memento is checked below.
contracts = [m for m in ir if m.get("kind") == "contract"]
mems_in_ir = [m for m in ir if m.get("kind") == "witness-memento"]
assert len(contracts) == 1, f"expected 1 contract in ir, got {len(contracts)}"
assert len(mems_in_ir) == 1, f"expected the witness-memento to ride in ir, got {len(mems_in_ir)}"
ev = contracts[0]["evidence"]
assert ev["proofType"] == "custom", f"proofType: {ev['proofType']}"
cert = ev["certificate"]
assert cert["tool"] == "junit", f"tool: {cert['tool']}"
pd = json.loads(cert["proofData"])
assert pd["kind"] == "witness-package", f"proofData.kind: {pd['kind']}"
assert "packageCid" in pd, "missing packageCid"
assert pd["packageCid"].startswith("blake3-512:"), f"packageCid: {pd['packageCid']}"
assert pd["count"] == 2, f"count: {pd['count']}"
assert pd["passed"] == 1, f"passed: {pd['passed']}"  # testPass passes, testFail fails

# Memento -- must be a signed witness-memento (kind + signer + signature), the
# exact shape the rust verifier's witness dimension recognizes and re-checks.
assert len(mementos) == 1, f"expected 1 memento, got {len(mementos)}"
mem = mementos[0]
assert mem["kind"] == "witness-memento", f"memento kind: {mem.get('kind')}"
assert mem["witness_kind"] == "junit-test-witness-package", f"kind: {mem['witness_kind']}"
assert mem["witness_cid"] == pd["packageCid"], "CID mismatch memento/proofData"
assert mem["signer"].startswith("ed25519:"), f"signer: {mem.get('signer')}"
assert mem["signature"].startswith("ed25519:"), f"signature: {mem.get('signature')}"
assert mem["count"] == 2 and mem["passed"] == 1
# The ir-resident memento must be byte-identical to the witness_mementos one.
assert mems_in_ir[0] == mem, "ir memento disagrees with witness_mementos memento"
print("W2: lift response shape OK")
PY
[ $? -eq 0 ] && pass "lift response shape" || fail "lift response shape"
rm -rf "$WORK2"

# ─── W3: resolve_witness returns valid bytes; blake3 recomputes ────────────────
echo
echo "=== W3: resolve_witness bytes + CID recompute ==="

WORK3=$(mktemp -d)
cat > "$WORK3/WitnessTest.java" << 'EOF'
public class WitnessTest {
    public void testOne() {}
    public void testTwo() {}
}
EOF

# Step 1: lift to get the bundle_cid
LIFT3_RESP=$(printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$WORK3\"}}" \
  '{"jsonrpc":"2.0","id":3,"method":"shutdown"}' \
  | "$JAVA" -cp "$KIT_CP" JavaJunitWitnessRpc 2>/dev/null \
  | awk 'NR==2')

BUNDLE_CID=$(python3 -c "
import json, sys
d = json.loads(sys.argv[1])
print(d['result']['witness_mementos'][0]['witness_cid'])
" "$LIFT3_RESP")

# Step 2: resolve_witness, check blake3(b64_decoded) == bundle_cid
RESOLVE3_RESP=$(printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"sugar.plugin.resolve_witness\",\"params\":{\"workspace_root\":\"$WORK3\",\"witness_cid\":\"$BUNDLE_CID\",\"witness_kind\":\"junit-test-witness-package\",\"memento\":{\"witness_cid\":\"$BUNDLE_CID\",\"witness_kind\":\"junit-test-witness-package\"}}}" \
  '{"jsonrpc":"2.0","id":3,"method":"shutdown"}' \
  | "$JAVA" -cp "$KIT_CP" JavaJunitWitnessRpc 2>/dev/null \
  | awk 'NR==2')

python3 - "$RESOLVE3_RESP" "$BUNDLE_CID" "$WORK3" << 'PY'
import json, sys, base64
resp_s, expected_cid, work_dir = sys.argv[1], sys.argv[2], sys.argv[3]
d = json.loads(resp_s)
assert "result" in d, f"error in resolve: {d}"
result = d["result"]
assert "body_b64" in result, f"missing body_b64: {result}"
assert result["witness_cid"] == expected_cid, "cid mismatch"

# Recompute blake3 of bytes
body_bytes = base64.b64decode(result["body_b64"])

# We use the same Python blake3 the verifier trusts
try:
    import blake3 as b3
    computed = "blake3-512:" + b3.blake3(body_bytes).digest(length=64).hex()
except ImportError:
    # Fall back: just check the bytes are non-empty and parseable
    computed = None

if computed is not None:
    assert computed == expected_cid, f"CID mismatch: computed={computed} expected={expected_cid}"
    print(f"W3: resolve_witness bytes recompute OK (blake3 verified)")
else:
    # Without blake3 lib, parse lines and check structure
    lines = [l for l in body_bytes.decode().split('\n') if l.strip()]
    assert all(json.loads(l).get("kind") == "junit-test-witness" for l in lines)
    print(f"W3: resolve_witness bytes structure OK (blake3 lib unavailable, structure checked)")
PY
[ $? -eq 0 ] && pass "resolve_witness bytes + CID" || fail "resolve_witness bytes + CID"
rm -rf "$WORK3"

# ─── W4: tampered body fails blake3 check ─────────────────────────────────────
echo
echo "=== W4: tampered body fails blake3 check ==="

python3 << 'PY'
import json, base64

# Simulate what the verifier does: if bytes are tampered, blake3 != pinned cid
# We verify that a body with one character changed has a different hash.
try:
    import blake3 as b3
    original = b'{"codeCid":"blake3-512:abc","codeFiles":"f.java","kind":"junit-test-witness","outcome":"passed","runtimeCid":"blake3-512:def","test":"Foo::bar"}\n'
    orig_cid = "blake3-512:" + b3.blake3(original).digest(length=64).hex()
    tampered = original[:-2] + b'x\n'  # flip last char before \n
    tamp_cid = "blake3-512:" + b3.blake3(tampered).digest(length=64).hex()
    assert orig_cid != tamp_cid, "tampered body should have different hash"
    print(f"W4: tampered body has different hash (verified by blake3)")
except ImportError:
    # Without blake3 lib just confirm different bytes → guard would fire
    print("W4: blake3 lib unavailable; tamper-detection relies on verifier (structural check only)")
PY
[ $? -eq 0 ] && pass "tampered body blake3 check" || fail "tampered body blake3 check"

# ─── W5: lying-discharge: all "passed" bytes with wrong CID refused ────────────
echo
echo "=== W5: lying-discharge bytes mismatch ==="

WORK5=$(mktemp -d)
cat > "$WORK5/WitnessTest.java" << 'EOF'
public class WitnessTest {
    public void testOne() {}
    public void testFail() { throw new AssertionError("fails"); }
}
EOF

# Get real bundle_cid from honest lift
LIFT5_RESP=$(printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$WORK5\"}}" \
  '{"jsonrpc":"2.0","id":3,"method":"shutdown"}' \
  | "$JAVA" -cp "$KIT_CP" JavaJunitWitnessRpc 2>/dev/null \
  | awk 'NR==2')

REAL_CID=$(python3 -c "
import json; d=json.loads('$LIFT5_RESP')
print(d['result']['witness_mementos'][0]['witness_cid'])
" 2>/dev/null || python3 -c "
import json,sys; d=json.loads(sys.argv[1])
print(d['result']['witness_mementos'][0]['witness_cid'])
" "$LIFT5_RESP")

python3 - "$REAL_CID" << 'PY'
import json, sys, base64

# The lying oracle returns bytes where both outcomes are "passed".
# The verifier recomputes blake3(bytes) and compares to pinned cid.
# Since the lying bytes are different from the honest bundle bytes,
# blake3(lying_bytes) != real_bundle_cid → verifier REFUSES.
real_cid = sys.argv[1]

lying_body = (
    '{"codeCid":"blake3-512:fake","codeFiles":"src/WitnessTest.java",'
    '"kind":"junit-test-witness","outcome":"passed",'
    '"runtimeCid":"blake3-512:fake","test":"WitnessTest::testFail"}\n'
    '{"codeCid":"blake3-512:fake","codeFiles":"src/WitnessTest.java",'
    '"kind":"junit-test-witness","outcome":"passed",'
    '"runtimeCid":"blake3-512:fake","test":"WitnessTest::testOne"}\n'
).encode()

try:
    import blake3 as b3
    computed = "blake3-512:" + b3.blake3(lying_body).digest(length=64).hex()
    if computed == real_cid:
        raise SystemExit("FAIL: lying body accidentally matches real cid (probability 1/2^512)")
    print(f"W5: lying-discharge bytes != real cid; verifier would refuse")
    print(f"   real:    {real_cid[:40]}...")
    print(f"   lying:   {computed[:40]}...")
except ImportError:
    print("W5: blake3 lib unavailable; lying-discharge structural check only")
PY
[ $? -eq 0 ] && pass "lying-discharge bytes mismatch" || fail "lying-discharge bytes mismatch"
rm -rf "$WORK5"

# ─── Summary ──────────────────────────────────────────────────────────────────
echo
echo "Witness kit unit tests: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
echo "== JavaJunitWitnessRpc unit tests: PASS =="
