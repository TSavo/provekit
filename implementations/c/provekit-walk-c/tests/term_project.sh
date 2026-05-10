#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-c11-term-project"
EXAMPLE_DIR="$SCRIPT_DIR/../../../../menagerie/c11-language-signature/example"
FOO_C="$EXAMPLE_DIR/foo.c"
FOO_TERM="$EXAMPLE_DIR/foo.term.json"
FOO_CONTRACT="$EXAMPLE_DIR/foo.contract.json"
TMP_DIR="${TMPDIR:-/tmp}/provekit-c11-term-project-test.$$"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

mkdir -p "$TMP_DIR"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

"$BIN" "$FOO_C" --function foo --term > "$TMP_DIR/foo.term.json"
"$BIN" "$FOO_C" --function foo --contract > "$TMP_DIR/foo.contract.json"

python3 - "$FOO_TERM" "$TMP_DIR/foo.term.json" "$FOO_CONTRACT" "$TMP_DIR/foo.contract.json" <<'PY'
import json
import sys

expected_term = json.load(open(sys.argv[1]))
actual_term = json.load(open(sys.argv[2]))
expected_contracts = json.load(open(sys.argv[3]))
actual_contract = json.load(open(sys.argv[4]))

if actual_term != expected_term:
    raise SystemExit(
        "FAIL: foo term mismatch\nexpected="
        + json.dumps(expected_term, sort_keys=True)
        + "\nactual="
        + json.dumps(actual_term, sort_keys=True)
    )

expected_contract = next(
    d for d in expected_contracts if d.get("kind") == "function-contract" and d.get("fn_name") == "foo"
)
if actual_contract != expected_contract:
    raise SystemExit(
        "FAIL: foo projected contract mismatch\nexpected="
        + json.dumps(expected_contract, sort_keys=True)
        + "\nactual="
        + json.dumps(actual_contract, sort_keys=True)
    )
PY

cat > "$TMP_DIR/add.c" <<'EOF'
int add(int a, int b) { return a + b; }
EOF

"$BIN" "$TMP_DIR/add.c" --function add --term > "$TMP_DIR/add.term.json"
"$BIN" "$TMP_DIR/add.c" --function add --contract > "$TMP_DIR/add.contract.json"

python3 - "$TMP_DIR/add.term.json" "$TMP_DIR/add.contract.json" <<'PY'
import json
import sys

term = json.load(open(sys.argv[1]))
contract = json.load(open(sys.argv[2]))

if term["term_surface"] != "return(add(a, b))":
    raise SystemExit(f"FAIL: add term surface mismatch: {term['term_surface']}")

expected_term = {
    "kind": "op",
    "name": "return",
    "args": [
        {
            "kind": "op",
            "name": "add",
            "args": [
                {"kind": "var", "name": "a"},
                {"kind": "var", "name": "b"},
            ],
        }
    ],
}
if term["term"] != expected_term:
    raise SystemExit(
        "FAIL: add term tree mismatch\nactual=" + json.dumps(term["term"], sort_keys=True)
    )

expected_post = {
    "kind": "atomic",
    "name": "=",
    "args": [
        {"kind": "var", "name": "result"},
        {
            "kind": "ctor",
            "name": "+",
            "args": [
                {"kind": "var", "name": "a"},
                {"kind": "var", "name": "b"},
            ],
        },
    ],
}
if contract["fn_name"] != "add" or contract["post"] != expected_post:
    raise SystemExit(
        "FAIL: add projected contract mismatch\nactual="
        + json.dumps(contract, sort_keys=True)
    )
PY

echo "term-project-ok"
