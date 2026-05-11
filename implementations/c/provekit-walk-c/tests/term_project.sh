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

python3 - "$ROOT/implementations/c/provekit-walk-c/src/c11_cursor_dispatch.generated.c" <<'PY'
import re
import sys

source = open(sys.argv[1], encoding="utf-8").read()
fail_closed = re.findall(r'\{"c11:[^"]+",\s*"[^"]*",\s*0\}', source)
if fail_closed:
    raise SystemExit("FAIL: fail-closed serializer dispatch entries remain: " + ", ".join(fail_closed))
PY

python3 - "$ROOT/menagerie/c11-language-signature" <<'PY'
import json
import sys
from pathlib import Path

base = Path(sys.argv[1])
signature = json.load(open(base / "specs" / "language_signature_c11.spec.json", encoding="utf-8"))
if "arity_shapes" not in signature:
    raise SystemExit("FAIL: C11 language signature must declare arity_shapes")
arity_shapes = signature["arity_shapes"]
if not isinstance(arity_shapes, dict):
    raise SystemExit("FAIL: C11 language signature arity_shapes must be an object")

operation_names = []
for spec_name in signature["operations"]:
    spec = json.load(open(base / "specs" / spec_name, encoding="utf-8"))
    operation_names.append(spec["fn_name"])
missing_shapes = sorted(set(operation_names) - set(arity_shapes))
extra_shapes = sorted(set(arity_shapes) - set(operation_names))
if missing_shapes or extra_shapes:
    raise SystemExit(
        "FAIL: C11 language signature arity_shapes must exactly cover operations: "
        + json.dumps({"missing": missing_shapes, "extra": extra_shapes}, sort_keys=True)
    )
for op_name in operation_names:
    shape = arity_shapes[op_name]
    if not isinstance(shape, dict) or shape.get("kind") not in {"named", "positional", "set"}:
        raise SystemExit(f"FAIL: {op_name} has invalid arity_shape {shape}")
    spec_path = "op_" + op_name.removeprefix("c11:").replace("-", "_") + ".spec.json"
    spec = json.load(open(base / "specs" / spec_path, encoding="utf-8"))
    spec_shape = spec.get("post", {}).get("arity_shape")
    if spec_shape is None:
        raise SystemExit(f"FAIL: {op_name} spec is missing post.arity_shape")
    if spec_shape != shape:
        raise SystemExit(
            f"FAIL: {op_name} spec arity_shape differs from signature arity_shapes entry: "
            + json.dumps({"spec": spec_shape, "signature": shape}, sort_keys=True)
        )

mapping = json.load(open(base / "cursor-kind-map.generated.json", encoding="utf-8"))
rows = {row["cursor_kind"]: row for row in mapping["rows"]}
binary_tokens = {
    token["token"]: token
    for token in rows["CXCursor_BinaryOperator"]["operator_dispatch"]["tokens"]
}
compound_tokens = {
    token["token"]: token
    for token in rows["CXCursor_CompoundAssignOperator"]["operator_dispatch"]["tokens"]
}

def require_token_shape(tokens, token, op_name, shape):
    entry = tokens[token]
    if entry["op_name"] != op_name:
        raise SystemExit(f"FAIL: {token} dispatches to {entry['op_name']}, expected {op_name}")
    if entry["arity_shape"] != shape:
        raise SystemExit(f"FAIL: {token} shape {entry['arity_shape']} != {shape}")

require_token_shape(binary_tokens, "+", "c11:bop_add", {"kind": "set"})
require_token_shape(binary_tokens, "&&", "c11:bop_logand", {
    "kind": "named",
    "slots": [{"name": "left"}, {"name": "right"}],
})
require_token_shape(binary_tokens, ",", "c11:bop_comma", {
    "kind": "named",
    "slots": [{"name": "first"}, {"name": "second"}],
})
require_token_shape(compound_tokens, "+=", "c11:compound_assign_add", {
    "kind": "named",
    "slots": [{"name": "lvalue"}, {"name": "rvalue"}],
})

generic_shape = rows["CXCursor_GenericSelectionExpr"]["arity_shape"]
if generic_shape != {
    "kind": "named",
    "slots": [
        {"name": "controlling", "evaluation": "unevaluated"},
        {"name": "branches", "shape": {"kind": "set"}},
    ],
}:
    raise SystemExit(f"FAIL: _Generic shape is not slot-evaluated: {generic_shape}")

asm_shape = rows["CXCursor_GCCAsmStmt"]["arity_shape"]
if rows["CXCursor_GCCAsmStmt"]["op_name"] != "c11:asm-link-edge":
    raise SystemExit("FAIL: GCC asm must dispatch to the asm link-edge op")
if asm_shape != {
    "kind": "named",
    "slots": [
        {"name": "path_cid", "slot_sort": "identifier"},
        {"name": "assembly_cid", "slot_sort": "identifier"},
        {"name": "target_surface", "slot_sort": "identifier"},
        {"name": "target_lifter", "slot_sort": "identifier"},
        {"name": "target_symbol", "slot_sort": "identifier"},
        {"name": "dialect", "slot_sort": "identifier"},
        {"name": "template", "slot_sort": "literal"},
        {"name": "assembly_source", "slot_sort": "literal"},
        {"name": "outputs", "shape": {"kind": "set"}},
        {"name": "inputs", "shape": {"kind": "set"}},
        {"name": "clobbers", "shape": {"kind": "set", "member_sort": "identifier"}},
    ],
}:
    raise SystemExit(f"FAIL: asm link-edge shape does not preserve x86-64 path slots: {asm_shape}")

spec = json.load(open(base / "specs" / "op_bop_add.spec.json", encoding="utf-8"))
if spec["post"].get("arity_shape") != {"kind": "set"}:
    raise SystemExit("FAIL: c11:bop_add spec must declare Set arity_shape")
asm_spec = json.load(open(base / "specs" / "op_asm_link_edge.spec.json", encoding="utf-8"))
if asm_spec["post"].get("linkage") != "link-edge":
    raise SystemExit("FAIL: asm op must declare linker-visible link-edge semantics")
if asm_spec["post"].get("target_surface") != "x86-64:sysv":
    raise SystemExit("FAIL: asm op must target the x86-64 SysV lifter surface")
if asm_spec["post"].get("target_lifter") != "provekit-lift-asm-x86-64":
    raise SystemExit("FAIL: asm op must name the x86-64 asm lifter")
PY

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

    if [ -f "$expected_contract" ]; then
        "$BIN" "$src" --function "$name" --contract > "$actual_contract"
    fi

    python3 - "$expected_term" "$actual_term" "$expected_contract" "$actual_contract" "$name" <<'PY'
import json
import sys
from pathlib import Path

expected_term = json.load(open(sys.argv[1]))
actual_term = json.load(open(sys.argv[2]))
expected_contract_path = Path(sys.argv[3])
actual_contract_path = Path(sys.argv[4])
name = sys.argv[5]

if actual_term != expected_term:
    raise SystemExit(
        f"FAIL: {name} term mismatch\nexpected="
        + json.dumps(expected_term, sort_keys=True)
        + "\nactual="
        + json.dumps(actual_term, sort_keys=True)
    )
if expected_contract_path.exists():
    expected_contract = json.load(open(expected_contract_path))
    actual_contract = json.load(open(actual_contract_path))
    if isinstance(expected_contract, list):
        expected_contract = next(
            d for d in expected_contract
            if d.get("kind") == "function-contract" and d.get("fn_name") == name
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

check_asm_orp_roundtrip() {
    name="asm_link"
    src="$EXAMPLE_DIR/$name.c"
    initial_term="$TMP_DIR/$name.initial.term.json"
    initial_term_cid_input="$TMP_DIR/$name.initial.term-cid-input.json"
    initial_identity_input="$TMP_DIR/$name.initial.identity-cid-input.json"
    target_symbol="$TMP_DIR/$name.target-symbol.txt"
    serial_c="$TMP_DIR/$name.roundtrip.c"
    roundtrip_term="$TMP_DIR/$name.roundtrip.term.json"
    roundtrip_term_cid_input="$TMP_DIR/$name.roundtrip.term-cid-input.json"
    roundtrip_identity_input="$TMP_DIR/$name.roundtrip.identity-cid-input.json"

    "$BIN" "$src" --function "$name" --term > "$initial_term"

    python3 - "$initial_term" "$TMP_DIR/$name.linked.s" "$target_symbol" <<'PY'
import json
import sys

term = json.load(open(sys.argv[1]))

def find_asm_link(node):
    if isinstance(node, dict):
        if node.get("kind") == "op" and node.get("name") == "asm-link-edge":
            return node
        for value in node.values():
            found = find_asm_link(value)
            if found is not None:
                return found
    elif isinstance(node, list):
        for item in node:
            found = find_asm_link(item)
            if found is not None:
                return found
    return None

edge = find_asm_link(term)
if edge is None:
    raise SystemExit("FAIL: asm_link did not lift to c11:asm-link-edge")
args = edge.get("args", [])
if len(args) != 11:
    raise SystemExit(f"FAIL: asm-link-edge arity {len(args)} != 11")
if args[2].get("name") != "x86-64:sysv":
    raise SystemExit(f"FAIL: target surface {args[2]} != x86-64:sysv")
if args[3].get("name") != "provekit-lift-asm-x86-64":
    raise SystemExit(f"FAIL: target lifter {args[3]} != provekit-lift-asm-x86-64")
symbol = args[4].get("name")
if not symbol or not symbol.startswith("provekit_inline_asm_"):
    raise SystemExit(f"FAIL: target symbol is not an inline asm link symbol: {args[4]}")
source = args[7].get("value")
if not source or symbol not in source or "nop" not in source:
    raise SystemExit(f"FAIL: assembly_source is not x86 lifter input: {source!r}")
open(sys.argv[2], "w", encoding="utf-8").write(source)
open(sys.argv[3], "w", encoding="utf-8").write(symbol)
PY

    (
        cd "$ROOT/implementations/rust"
        cargo run -q -p provekit-lift-asm-x86-64 -- --rpc <<EOF > "$TMP_DIR/$name.x86.jsonl"
{"jsonrpc":"2.0","id":1,"method":"lift","params":{"surface":"x86-64:sysv","workspace_root":".","source_paths":["$TMP_DIR/$name.linked.s"]}}
EOF
    )

    python3 - "$TMP_DIR/$name.x86.jsonl" "$target_symbol" <<'PY'
import json
import sys

expected_symbol = open(sys.argv[2], encoding="utf-8").read().strip()
lines = [line for line in open(sys.argv[1], encoding="utf-8").read().splitlines() if line.strip()]
if not lines:
    raise SystemExit("FAIL: x86 lifter emitted no RPC response")
response = json.loads(lines[-1])
if "error" in response:
    raise SystemExit("FAIL: x86 lifter refused C-emitted asm: " + json.dumps(response["error"], sort_keys=True))
decls = response.get("result", {}).get("declarations", [])
if not any(d.get("fnName") == expected_symbol for d in decls):
    raise SystemExit(f"FAIL: x86 lifter did not emit a contract linked to {expected_symbol}")
PY

    "$BIN" --serialize "$initial_term" --function "$name" > "$serial_c"
    "$BIN" "$serial_c" --function "$name" --term > "$roundtrip_term"

    python3 - "$initial_term" "$roundtrip_term" "$initial_term_cid_input" "$roundtrip_term_cid_input" <<'PY'
import json
import sys

before = json.load(open(sys.argv[1]))
after = json.load(open(sys.argv[2]))
if before["source_cid"] != after["source_cid"]:
    raise SystemExit(
        "FAIL: c-with-asm ORP roundtrip changed the raw C source CID\n"
        + json.dumps({"before": before["source_cid"], "after": after["source_cid"]}, sort_keys=True)
    )
if before["signature_cid"] != after["signature_cid"] or before["term"] != after["term"]:
    raise SystemExit(
        "FAIL: c-with-asm ORP roundtrip changed the C asm link term CID inputs\n"
        + json.dumps({"before": before, "after": after}, sort_keys=True)
    )
before_input = {"signature_cid": before["signature_cid"], "term": before["term"]}
after_input = {"signature_cid": after["signature_cid"], "term": after["term"]}
open(sys.argv[3], "w", encoding="utf-8").write(json.dumps(before_input, sort_keys=True, separators=(",", ":")))
open(sys.argv[4], "w", encoding="utf-8").write(json.dumps(after_input, sort_keys=True, separators=(",", ":")))
PY

    initial_term_cid="$(
        cd "$ROOT/implementations/rust"
        cargo run -q -p provekit-canonicalizer --bin compute_fixture_cid -- "$initial_term_cid_input"
    )"
    roundtrip_term_cid="$(
        cd "$ROOT/implementations/rust"
        cargo run -q -p provekit-canonicalizer --bin compute_fixture_cid -- "$roundtrip_term_cid_input"
    )"
    if [ "$initial_term_cid" != "$roundtrip_term_cid" ]; then
        echo "FAIL: c-with-asm ORP roundtrip changed term CID: $initial_term_cid != $roundtrip_term_cid" >&2
        exit 1
    fi

    python3 - "$initial_term" "$roundtrip_term" "$initial_identity_input" "$roundtrip_identity_input" "$initial_term_cid" "$roundtrip_term_cid" <<'PY'
import json
import sys

before = json.load(open(sys.argv[1]))
after = json.load(open(sys.argv[2]))
before_input = {
    "source_cid": before["source_cid"],
    "signature_cid": before["signature_cid"],
    "term_cid": sys.argv[5],
}
after_input = {
    "source_cid": after["source_cid"],
    "signature_cid": after["signature_cid"],
    "term_cid": sys.argv[6],
}
open(sys.argv[3], "w", encoding="utf-8").write(json.dumps(before_input, sort_keys=True, separators=(",", ":")))
open(sys.argv[4], "w", encoding="utf-8").write(json.dumps(after_input, sort_keys=True, separators=(",", ":")))
PY

    if ! cmp -s "$initial_identity_input" "$roundtrip_identity_input"; then
        echo "FAIL: c-with-asm ORP roundtrip changed canonical identity bytes" >&2
        diff -u "$initial_identity_input" "$roundtrip_identity_input" >&2 || true
        exit 1
    fi

    initial_cid="$(
        cd "$ROOT/implementations/rust"
        cargo run -q -p provekit-canonicalizer --bin compute_fixture_cid -- "$initial_identity_input"
    )"
    roundtrip_cid="$(
        cd "$ROOT/implementations/rust"
        cargo run -q -p provekit-canonicalizer --bin compute_fixture_cid -- "$roundtrip_identity_input"
    )"
    if [ "$initial_cid" != "$roundtrip_cid" ]; then
        echo "FAIL: c-with-asm ORP roundtrip changed identity CID: $initial_cid != $roundtrip_cid" >&2
        exit 1
    fi
}

check_example foo
check_example add
check_example g
check_example loop
check_example call
check_example cond
check_example lit
check_example control
check_example gnu
check_example asm_link
check_asm_orp_roundtrip

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
    "bop_div",
    "bop_mod",
    "bop_shl",
    "bop_shr",
    "bop_bitand",
    "bop_bitor",
    "bop_bitxor",
    "bop_gt",
    "bop_ge",
    "bop_ne",
    "uop_bitnot",
    "uop_addr_of",
    "uop_pre_inc",
    "uop_post_inc",
    "uop_pre_dec",
    "uop_post_dec",
    "uop_plus",
    "compound_assign_add",
    "compound_assign_div",
    "compound_assign_mod",
    "compound_assign_shl",
    "compound_assign_shr",
    "compound_assign_bitand",
    "compound_assign_bitor",
    "compound_assign_bitxor",
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
