#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Integration test for provekit-lsp-cpp.
#
# Tests shared LSP protocol: initialize, analyzeDocument, plus legacy lift,
# parse, shutdown.
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
int add(int a, int b) { return a + b; }

//provekit:contract
int compute_sum(int a, int b) { return add(a, b); }
EOF

echo "=== initialize ==="
INIT_INPUT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
INIT_RESP=$(printf '%s\n{"jsonrpc":"2.0","id":2,"method":"shutdown"}\n' "$INIT_INPUT" | "$BIN" | head -1)
assert_contains "name field"              "$INIT_RESP" '"name":"provekit-lsp-cpp"'
assert_contains "version field"           "$INIT_RESP" '"version":"0.1.0"'
assert_contains "shared protocol"         "$INIT_RESP" '"protocol_version":"provekit-lsp-shared/1"'
assert_contains "kit id"                  "$INIT_RESP" '"kit_id":"cpp"'
assert_contains "protocol catalog"        "$INIT_RESP" '"protocol_catalog_cid":"blake3-512:'
assert_contains "source surface"          "$INIT_RESP" '"source_surfaces":["cpp-source"]'
assert_contains "implication code"        "$INIT_RESP" '"provekit.lsp.implication_failed"'
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
assert_contains "callEdges target"        "$LIFT_RESP" '"targetSymbol":"cpp-kit:add"'
assert_contains "callEdges source"        "$LIFT_RESP" '"sourceContractCid":"pending-cpp:compute_sum"'
assert_contains "callEdges locus"         "$LIFT_RESP" '"callSiteLocus":{'
assert_contains "diagnostics key"         "$LIFT_RESP" '"diagnostics":[]'
assert_not_contains "no error"            "$LIFT_RESP" '"error"'

echo "=== parse (legacy) ==="
PARSE_INPUT=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"fixture.cpp","source":"//provekit:contract\nint add(int a, int b) { return a + b; }\n\n//provekit:contract\nint compute_sum(int a, int b) { return add(a, b); }"}}' \
    '{"jsonrpc":"2.0","id":3,"method":"shutdown"}')
PARSE_OUTPUT=$(printf '%s\n' "$PARSE_INPUT" | "$BIN")
PARSE_RESP=$(printf '%s\n' "$PARSE_OUTPUT" | sed -n '2p')
assert_contains "declarations field"      "$PARSE_RESP" '"declarations":'
assert_contains "callEdges target"        "$PARSE_RESP" '"targetSymbol":"cpp-kit:add"'
assert_contains "callEdges source"        "$PARSE_RESP" '"sourceContractCid":"pending-cpp:compute_sum"'
assert_contains "callEdges locus"         "$PARSE_RESP" '"callSiteLocus":{'
assert_contains "at least one decl"       "$PARSE_RESP" '"kind":"contract"'
assert_contains "contract name"           "$PARSE_RESP" '"name":"compute_sum"'
assert_not_contains "no error"            "$PARSE_RESP" '"error"'

echo "=== analyzeDocument ==="
ANALYZE_INPUT=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    '{"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"kit_id":"cpp","uri":"file:///project/FloorFixture.cpp","file":"FloorFixture.cpp","text":"// Forward-propagation floor fixture for C++\n// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback\n\nbool checkPositive(int x) {\n    if (x <= 0) { return false; }  // pre: x > 0\n    return true;\n}\n\nbool callerSatisfiesPre() {\n    bool result = checkPositive(5);  // satisfies pre (x=5 > 0)\n    return result;\n}\n\nbool callerViolatesPre() {\n    bool result = checkPositive(-1);  // violates pre (x=-1 <= 0)\n    return result;\n}\n\nbool callerWithLoop() {\n    for (int i = 0; i < 10; i++) {\n        bool result = checkPositive(i);  // top fallback at loop entry\n        if (!result) { return false; }\n    }\n    return true;\n}\n","document_version":42,"workspace_root":"/project","accepted_protocol_catalog_cids":[],"policy_cids":[]}}' \
    '{"jsonrpc":"2.0","id":3,"method":"shutdown"}')
ANALYZE_OUTPUT=$(printf '%s\n' "$ANALYZE_INPUT" | "$BIN")
ANALYZE_RESP=$(printf '%s\n' "$ANALYZE_OUTPUT" | sed -n '2p')
assert_contains "analysis kind"           "$ANALYZE_RESP" '"kind":"lsp-document-analysis"'
assert_contains "analysis kit"            "$ANALYZE_RESP" '"kit_id":"cpp"'
assert_contains "document cid"            "$ANALYZE_RESP" '"document_cid":"blake3-512:'
assert_contains "diagnostic code"         "$ANALYZE_RESP" '"code":"provekit.lsp.implication_failed"'
assert_contains "diagnostic severity"     "$ANALYZE_RESP" '"severity":"error"'
assert_contains "diagnostic producer"     "$ANALYZE_RESP" '"producer":"forward-propagation"'
assert_contains "diagnostic callee"       "$ANALYZE_RESP" '"callee":"checkPositive"'
assert_contains "diagnostic line"         "$ANALYZE_RESP" '"start_line":15'
assert_contains "diagnostic column"       "$ANALYZE_RESP" '"start_col":18'
assert_not_contains "no rpc error"        "$ANALYZE_RESP" '"error":'

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
