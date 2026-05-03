#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Integration test for provekit-lsp-c.
#
# Spawns the binary, pipes JSON-RPC requests, asserts JSON responses
# match the expected shape. Fixture: tests/fixtures/two_funcs.c
# (2 functions, 1 call site).

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lsp-c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    echo "      Run 'make' first." >&2
    exit 1
fi

FIXTURE="$SCRIPT_DIR/fixtures/two_funcs.c"
SOURCE=$(cat "$FIXTURE" | sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' | tr -d '\n' | sed 's/\\n$//')

PASS=0
FAIL=0

check() {
    local label="$1"
    local response="$2"
    local must_contain="$3"

    if printf '%s' "$response" | grep -qF "$must_contain"; then
        printf "  PASS: %s\n" "$label"
        PASS=$((PASS + 1))
    else
        printf "  FAIL: %s\n" "$label" >&2
        printf "        expected to contain: %s\n" "$must_contain" >&2
        printf "        got: %s\n" "$response" >&2
        FAIL=$((FAIL + 1))
    fi
}

# Build the NDJSON request sequence.
# We embed the fixture source inline so no file I/O is needed from the binary.
REQUESTS=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"parse\",\"params\":{\"path\":\"two_funcs.c\",\"source\":\"$SOURCE\"}}" \
    '{"jsonrpc":"2.0","id":3,"method":"shutdown"}')

# Pipe all requests at once; collect all responses.
RESPONSES=$(printf '%s\n' "$REQUESTS" | "$BIN" --rpc)

LINE1=$(printf '%s\n' "$RESPONSES" | sed -n '1p')
LINE2=$(printf '%s\n' "$RESPONSES" | sed -n '2p')
LINE3=$(printf '%s\n' "$RESPONSES" | sed -n '3p')

printf "Running provekit-lsp-c integration tests...\n"

# T1: initialize response contains name
check "T1 initialize: name" "$LINE1" '"name":"provekit-lsp-c"'

# T2: initialize response contains version
check "T2 initialize: version" "$LINE1" '"version":"0.1.0"'

# T3: initialize response contains capabilities array with parse
check "T3 initialize: capabilities contains parse" "$LINE1" '"parse"'

# T4: parse response contains declarations array
check "T4 parse: declarations key present" "$LINE2" '"declarations":'

# T5: parse response declares function 'add'
check "T5 parse: function add declared" "$LINE2" '"add"'

# T6: parse response declares function 'compute'
check "T6 parse: function compute declared" "$LINE2" '"compute"'

# T7: parse response contains callEdges array
check "T7 parse: callEdges key present" "$LINE2" '"callEdges":'

# T8: parse response has call edge from compute to add
check "T8 parse: call edge compute->add present" "$LINE2" '"callee":"add"'

# T9: parse response contains warnings array
check "T9 parse: warnings key present" "$LINE2" '"warnings":'

# T10: shutdown response contains null result
check "T10 shutdown: result null" "$LINE3" '"result":null'

printf "\nResults: %d passed, %d failed\n" "$PASS" "$FAIL"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
