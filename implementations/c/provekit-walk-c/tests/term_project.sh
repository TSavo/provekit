#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
BIN="$SCRIPT_DIR/../provekit-c11-term-project"
EXAMPLE_DIR="$ROOT/menagerie/c11-language-signature/example"
TMP_DIR="${TMPDIR:-/tmp}/provekit-c11-term-project-test.$$"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

mkdir -p "$TMP_DIR"

python3 "$ROOT/tools/generate-c11-from-cursorkind.py" --check

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

check_example() {
    name="$1"
    src="$EXAMPLE_DIR/$name.c"
    expected_term="$EXAMPLE_DIR/$name.term.json"
    expected_contract="$EXAMPLE_DIR/$name.contract.json"
    actual_term="$TMP_DIR/$name.term.json"
    actual_contract="$TMP_DIR/$name.contract.json"
    serial_c="$TMP_DIR/$name.roundtrip.c"
    roundtrip_term="$TMP_DIR/$name.roundtrip.term.json"

    "$BIN" "$src" --function "$name" --term > "$actual_term"
    "$BIN" "$src" --function "$name" --contract > "$actual_contract"

    python3 - "$expected_term" "$actual_term" "$expected_contract" "$actual_contract" "$name" <<'PY'
import json
import sys

expected_term = json.load(open(sys.argv[1]))
actual_term = json.load(open(sys.argv[2]))
expected_contract = json.load(open(sys.argv[3]))
actual_contract = json.load(open(sys.argv[4]))
name = sys.argv[5]

if isinstance(expected_contract, list):
    expected_contract = next(
        d for d in expected_contract
        if d.get("kind") == "function-contract" and d.get("fn_name") == name
    )

if actual_term != expected_term:
    raise SystemExit(
        f"FAIL: {name} term mismatch\nexpected="
        + json.dumps(expected_term, sort_keys=True)
        + "\nactual="
        + json.dumps(actual_term, sort_keys=True)
    )
if actual_contract != expected_contract:
    raise SystemExit(
        f"FAIL: {name} projected contract mismatch\nexpected="
        + json.dumps(expected_contract, sort_keys=True)
        + "\nactual="
        + json.dumps(actual_contract, sort_keys=True)
    )
PY

    "$BIN" --serialize "$actual_term" --function "$name" > "$serial_c"
    "$BIN" "$serial_c" --function "$name" --term > "$roundtrip_term"

    python3 - "$actual_term" "$roundtrip_term" "$name" <<'PY'
import json
import sys

before = json.load(open(sys.argv[1]))
after = json.load(open(sys.argv[2]))
name = sys.argv[3]

if before["signature_cid"] != after["signature_cid"] or before["term"] != after["term"]:
    raise SystemExit(
        f"FAIL: {name} parse/serialize/parse changed the term CID inputs\n"
        + json.dumps({"before": before, "after": after}, sort_keys=True)
    )
PY
}

check_example foo
check_example add
check_example g

operator_src="$TMP_DIR/operator_ops.c"
operator_term="$TMP_DIR/operator_ops.term.json"
cat > "$operator_src" <<'C'
static int operator_ops(int a, int b, int *p) {
    int x = a / b;
    x = x % b;
    x = x << 1;
    x = x >> 1;
    x = x & b;
    x = x | b;
    x = x ^ b;
    if (x > b)
        x = x + 1;
    if (x >= b)
        x = x + 1;
    if (x != b)
        x = x + 1;
    x = ~x;
    x = +x;
    x = *p;
    int *q = &x;
    ++x;
    x++;
    --x;
    x--;
    x += b;
    x /= b;
    x %= b;
    x <<= 1;
    x >>= 1;
    x &= b;
    x |= b;
    x ^= b;
    return x + *q;
}
C

"$BIN" "$operator_src" --function operator_ops --term > "$operator_term"

python3 - "$operator_term" <<'PY'
import json
import sys

term = json.load(open(sys.argv[1]))["term"]
names = set()

def walk(node):
    if isinstance(node, dict):
        if node.get("kind") == "op":
            names.add(node.get("name"))
        for value in node.values():
            walk(value)
    elif isinstance(node, list):
        for item in node:
            walk(item)

walk(term)
expected = {
    "div",
    "mod",
    "shl",
    "shr",
    "bit_and",
    "bit_or",
    "bit_xor",
    "gt",
    "ge",
    "ne",
    "bit_not",
    "addr_of",
    "pre_inc",
    "post_inc",
    "pre_dec",
    "post_dec",
    "plus",
}
missing = sorted(expected - names)
fallbacks = sorted({"binary-operator", "unary-operator"} & names)
if missing or fallbacks:
    raise SystemExit(
        "FAIL: missing concrete operator ops: "
        + ",".join(missing)
        + "; fallback ops present: "
        + ",".join(fallbacks)
    )
PY

python3 - "$EXAMPLE_DIR/g.term.json" <<'PY'
import json
import sys

term = json.load(open(sys.argv[1]))["term"]

def walk(node):
    if isinstance(node, dict):
        if node.get("kind") == "op" and node.get("name") == "opaque":
            raise SystemExit("FAIL: g term contains c11:opaque")
        for value in node.values():
            walk(value)
    elif isinstance(node, list):
        for item in node:
            walk(item)

walk(term)
PY

echo "term-project-ok"
