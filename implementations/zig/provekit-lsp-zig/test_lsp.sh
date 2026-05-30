#!/bin/sh
# Integration test for provekit-lsp-zig.
#
# Lifecycle: initialize -> parse -> analyzeDocument -> shutdown.
# Asserts:
#   1. initialize response has the shared LSP protocol shape.
#   2. parse response contains declarations array, callEdges array, warnings array
#   3. declarations is non-empty (at least one contract lifted from fixture)
#   4. callEdges contains a canonical call edge with callSiteLocus
#   5. analyzeDocument returns an lsp-document-analysis callsite diagnostic
#   6. shutdown response result is null
#
# Usage: sh test_lsp.sh [path-to-binary]

set -e

WORKSPACE="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="${1:-$WORKSPACE/implementations/zig/provekit-lsp-zig/zig-out/bin/provekit-lsp-zig}"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found or not executable: $BIN" >&2
    echo "  Build with: cd implementations/zig/provekit-lsp-zig && zig build" >&2
    exit 1
fi

PASS=0
FAIL=0

assert_contains() {
    label="$1"
    text="$2"
    pattern="$3"
    if echo "$text" | grep -qF "$pattern"; then
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
    if echo "$text" | grep -qF "$pattern"; then
        echo "  FAIL: $label (should NOT contain: $pattern)"
        echo "    got: $text"
        FAIL=$((FAIL + 1))
    else
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    fi
}

# Fixture: a tiny Zig source with two native function bodies and one call site.
INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
PARSE='{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"fixture.zig","source":"pub fn add(x: i64) i64 { return x; }\n\npub fn myFn(x: i64) i64 { return add(x); }"}}'
ANALYZE='{"jsonrpc":"2.0","id":3,"method":"analyzeDocument","params":{"kit_id":"zig","uri":"file:///project/FloorFixture.zig","file":"FloorFixture.zig","text":"pub fn checkPositive(x: i64) bool {\n    if (x <= 0) return false;\n    return true;\n}\n\npub fn callerSatisfiesPre() bool {\n    const result = checkPositive(5);\n    return result;\n}\n\npub fn callerViolatesPre() bool {\n    const result = checkPositive(-1);\n    return result;\n}\n\npub fn callerWithLoop() bool {\n    var i: i64 = 0;\n    while (i < 10) : (i += 1) {\n        const result = checkPositive(i);\n        if (!result) return false;\n    }\n    return true;\n}\n","document_version":42,"workspace_root":"/project","accepted_protocol_catalog_cids":[],"policy_cids":[]}}'
SHUTDOWN='{"jsonrpc":"2.0","id":4,"method":"shutdown","params":{}}'

INPUT="$(printf '%s\n%s\n%s\n%s\n' "$INIT" "$PARSE" "$ANALYZE" "$SHUTDOWN")"

OUTPUT="$(printf '%s' "$INPUT" | "$BIN")"

INIT_RESP="$(printf '%s' "$OUTPUT" | head -1)"
PARSE_RESP="$(printf '%s' "$OUTPUT" | sed -n '2p')"
ANALYZE_RESP="$(printf '%s' "$OUTPUT" | sed -n '3p')"
SHUTDOWN_RESP="$(printf '%s' "$OUTPUT" | sed -n '4p')"

echo "=== initialize ==="
assert_contains "name field"          "$INIT_RESP"     '"name":"provekit-lsp-zig"'
assert_contains "version field"       "$INIT_RESP"     '"version":"0.1.0"'
assert_contains "shared protocol"     "$INIT_RESP"     '"protocol_version":"provekit-lsp-shared/1"'
assert_contains "kit id"              "$INIT_RESP"     '"kit_id":"zig"'
assert_contains "protocol catalog"    "$INIT_RESP"     '"protocol_catalog_cid":"blake3-512:'
assert_contains "source surface"      "$INIT_RESP"     '"source_surfaces":["zig-source"]'
assert_contains "implication code"    "$INIT_RESP"     '"provekit.lsp.implication_failed"'
assert_not_contains "no error"        "$INIT_RESP"     '"error"'

echo "=== parse ==="
assert_contains "declarations field"  "$PARSE_RESP"    '"declarations":'
assert_contains "callEdges target"    "$PARSE_RESP"    '"targetSymbol":"zig-kit:add"'
assert_contains "callEdges source"    "$PARSE_RESP"    '"sourceContractCid":"pending-zig:myFn"'
assert_contains "callEdges locus"     "$PARSE_RESP"    '"callSiteLocus":{'
assert_contains "warnings field"      "$PARSE_RESP"    '"warnings":[]'
assert_contains "at least one decl"   "$PARSE_RESP"    '"kind":"function-contract"'
assert_contains "contract name"       "$PARSE_RESP"    '"fnName":"fixture.zig.myFn"'
assert_not_contains "no error"        "$PARSE_RESP"    '"error"'

echo "=== analyzeDocument ==="
assert_contains "analysis kind"       "$ANALYZE_RESP"  '"kind":"lsp-document-analysis"'
assert_contains "analysis kit"        "$ANALYZE_RESP"  '"kit_id":"zig"'
assert_contains "document cid"        "$ANALYZE_RESP"  '"document_cid":"blake3-512:'
assert_contains "diagnostic code"     "$ANALYZE_RESP"  '"code":"provekit.lsp.implication_failed"'
assert_contains "diagnostic severity" "$ANALYZE_RESP"  '"severity":"error"'
assert_contains "diagnostic producer" "$ANALYZE_RESP"  '"producer":"forward-propagation"'
assert_contains "diagnostic callee"   "$ANALYZE_RESP"  '"callee":"checkPositive"'
assert_contains "diagnostic line"     "$ANALYZE_RESP"  '"start_line":12'
assert_contains "diagnostic column"   "$ANALYZE_RESP"  '"start_col":19'
assert_not_contains "no rpc error"    "$ANALYZE_RESP"  '"error":'

echo "=== shutdown ==="
assert_contains "result null"         "$SHUTDOWN_RESP" '"result":null'
assert_not_contains "no error"        "$SHUTDOWN_RESP" '"error"'

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
