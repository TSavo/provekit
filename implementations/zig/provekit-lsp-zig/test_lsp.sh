#!/bin/sh
# Integration test for provekit-lsp-zig.
#
# Lifecycle: initialize -> parse (fixture with //provekit:contract) -> shutdown.
# Asserts:
#   1. initialize response has name="provekit-lsp-zig", version="0.1.0",
#      capabilities=["parse"]
#   2. parse response contains declarations array, callEdges array, warnings array
#   3. declarations is non-empty (at least one contract lifted from fixture)
#   4. callEdges is empty array []
#   5. shutdown response result is null
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

# Fixture: a tiny Zig source with one //provekit:contract annotation.
INIT='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
PARSE='{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"fixture.zig","source":"//provekit:contract\nfn myFn(x: i32) void { _ = x; }"}}'
SHUTDOWN='{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}'

INPUT="$(printf '%s\n%s\n%s\n' "$INIT" "$PARSE" "$SHUTDOWN")"

OUTPUT="$(printf '%s' "$INPUT" | "$BIN")"

INIT_RESP="$(printf '%s' "$OUTPUT" | head -1)"
PARSE_RESP="$(printf '%s' "$OUTPUT" | sed -n '2p')"
SHUTDOWN_RESP="$(printf '%s' "$OUTPUT" | sed -n '3p')"

echo "=== initialize ==="
assert_contains "name field"          "$INIT_RESP"     '"name":"provekit-lsp-zig"'
assert_contains "version field"       "$INIT_RESP"     '"version":"0.1.0"'
assert_contains "capabilities parse"  "$INIT_RESP"     '"capabilities":["parse"]'
assert_not_contains "no error"        "$INIT_RESP"     '"error"'

echo "=== parse ==="
assert_contains "declarations field"  "$PARSE_RESP"    '"declarations":'
assert_contains "callEdges field"     "$PARSE_RESP"    '"callEdges":[]'
assert_contains "warnings field"      "$PARSE_RESP"    '"warnings":[]'
assert_contains "at least one decl"   "$PARSE_RESP"    '"kind":"contract"'
assert_contains "contract name"       "$PARSE_RESP"    '"name":"myFn"'
assert_not_contains "no error"        "$PARSE_RESP"    '"error"'

echo "=== shutdown ==="
assert_contains "result null"         "$SHUTDOWN_RESP" '"result":null'
assert_not_contains "no error"        "$SHUTDOWN_RESP" '"error"'

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
