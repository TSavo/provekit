#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Integration test for provekit-lsp-cpp.
#
# Tests provekit-lift/1 protocol: initialize, lift, parse (legacy), shutdown.
#
# Usage: sh test_lsp.sh [path-to-binary]

set -e

WORKSPACE="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="${1:-$WORKSPACE/implementations/cpp/target/provekit-lsp-cpp}"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found or not executable: $BIN" >&2
    echo "  Build with: tools/build-cpp-lsp.sh" >&2
    exit 1
fi

PASS=0
FAIL=0

assert_contains() {
    label="$1"
    text="$2"
    pattern="$3"
    if printf '%s' "$text" | grep -qF "$pattern"; then
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $label"
        echo "    expected to contain: $pattern"
        echo "    got: $text"
        FAIL=$((FAIL + 1))
    fi
}

assert_not_contains() {
    label="$1"
    text="$2"
    pattern="$3"
    if printf '%s' "$text" | grep -qF "$pattern"; then
        echo "  FAIL: $label (should NOT contain: $pattern)"
        echo "    got: $text"
        FAIL=$((FAIL + 1))
    else
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    fi
}

# Create a temp directory with a fixture C++ file.
TMPDIR_PATH="$(mktemp -d)"
cat > "$TMPDIR_PATH/fixture.cpp" << 'EOF'
//provekit:contract
int compute_sum(int a, int b) { return a + b; }
EOF

echo "=== initialize ==="
INIT_INPUT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
INIT_RESP=$(printf '%s\n{"jsonrpc":"2.0","id":2,"method":"shutdown"}\n' "$INIT_INPUT" | "$BIN" | head -1)
assert_contains "name field"              "$INIT_RESP" '"name":"provekit-lsp-cpp"'
assert_contains "version field"           "$INIT_RESP" '"version":"0.1.0"'
assert_contains "protocol_version"        "$INIT_RESP" '"protocol_version":"provekit-lift/1"'
assert_contains "authoring_surfaces"      "$INIT_RESP" '"authoring_surfaces":["cpp-source"]'
assert_not_contains "no error"            "$INIT_RESP" '"error"'

echo "=== lift ==="
LIFT_INPUT=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$TMPDIR_PATH\",\"source_paths\":[\"fixture.cpp\"]}}" \
    '{"jsonrpc":"2.0","id":3,"method":"shutdown"}')
LIFT_OUTPUT=$(printf '%s\n' "$LIFT_INPUT" | "$BIN")
LIFT_RESP=$(printf '%s\n' "$LIFT_OUTPUT" | sed -n '2p')
assert_contains "kind ir-document"        "$LIFT_RESP" '"kind":"ir-document"'
assert_contains "ir key"                  "$LIFT_RESP" '"ir":'
assert_contains "contract compute_sum"    "$LIFT_RESP" '"name":"compute_sum"'
assert_contains "callEdges empty"         "$LIFT_RESP" '"callEdges":[]'
assert_contains "diagnostics key"         "$LIFT_RESP" '"diagnostics":[]'
assert_not_contains "no error"            "$LIFT_RESP" '"error"'

echo "=== parse (legacy) ==="
PARSE_INPUT=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"fixture.cpp","source":"//provekit:contract\nint compute_sum(int a, int b) { return a + b; }"}}' \
    '{"jsonrpc":"2.0","id":3,"method":"shutdown"}')
PARSE_OUTPUT=$(printf '%s\n' "$PARSE_INPUT" | "$BIN")
PARSE_RESP=$(printf '%s\n' "$PARSE_OUTPUT" | sed -n '2p')
assert_contains "declarations field"      "$PARSE_RESP" '"declarations":'
assert_contains "callEdges empty"         "$PARSE_RESP" '"callEdges":[]'
assert_contains "at least one decl"       "$PARSE_RESP" '"kind":"contract"'
assert_contains "contract name"           "$PARSE_RESP" '"name":"compute_sum"'
assert_not_contains "no error"            "$PARSE_RESP" '"error"'

echo "=== shutdown ==="
SHUTDOWN_RESP=$(printf '%s\n' "$LIFT_OUTPUT" | sed -n '3p')
assert_contains "result null"             "$SHUTDOWN_RESP" '"result":null'
assert_not_contains "no error"            "$SHUTDOWN_RESP" '"error"'

# Cleanup
rm -rf "$TMPDIR_PATH"

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
