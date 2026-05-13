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

# ---------------------------------------------------------------------------
# Round 1: initialize + parse + shutdown
# ---------------------------------------------------------------------------
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
printf "\n-- initialize / parse / shutdown --\n"

# T1: initialize response contains name
check "T1 initialize: name" "$LINE1" '"name":"provekit-lsp-c"'

# T2: initialize response contains version
check "T2 initialize: version" "$LINE1" '"version":"0.1.0"'

# T3: initialize response contains protocol_version
check "T3 initialize: protocol_version" "$LINE1" '"protocol_version":"provekit-lift/1"'

# T3b: initialize response contains authoring_surfaces
check "T3b initialize: authoring_surfaces c-source" "$LINE1" '"authoring_surfaces":["c-source"]'

# T4: parse response contains declarations array
check "T4 parse: declarations key present" "$LINE2" '"declarations":'

# T5: parse response declares contract 'add' with kind:contract shape
check "T5 parse: contract add declared" "$LINE2" '"kind":"contract"'

# T6: parse response declares contract 'add' by name
check "T6 parse: contract add named" "$LINE2" '"name":"add"'

# T7: parse response declares contract 'compute' by name
check "T7 parse: contract compute declared" "$LINE2" '"name":"compute"'

# T8: parse response contains callEdges array
check "T8 parse: callEdges key present" "$LINE2" '"callEdges":'

# T9: callEdges is emitted as an empty array.
#
# The C LSP cannot compute contract CIDs (no JCS encoder + BLAKE3 here), so
# the canonical IR shape (sourceContractCid, targetContractCid, targetSymbol,
# callSiteLocus, evidenceTerm) cannot be produced. Until that's wired up,
# emit []; the legacy {callee, caller, line} shape was silently dropped by
# the daemon. (Review feedback: PR #165 / Copilot.)
check "T9 parse: callEdges is empty array" "$LINE2" '"callEdges":[]'

# T10: parse response contains diagnostics array
check "T10 parse: diagnostics key present" "$LINE2" '"diagnostics":'

# T11: parse response contains opacityReport array
check "T11 parse: opacityReport key present" "$LINE2" '"opacityReport":'

# T12: parse response contains refusals array
check "T12 parse: refusals key present" "$LINE2" '"refusals":'

# T13: shutdown response contains null result
check "T13 shutdown: result null" "$LINE3" '"result":null'

# ---------------------------------------------------------------------------
# Round 2: lift method — pass the fixture path directly
# ---------------------------------------------------------------------------
printf "\n-- lift method --\n"

LIFT_REQUESTS=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":10,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$SCRIPT_DIR\",\"source_paths\":[\"$FIXTURE\"]}}" \
    '{"jsonrpc":"2.0","id":12,"method":"shutdown"}')

LIFT_RESPONSES=$(printf '%s\n' "$LIFT_REQUESTS" | "$BIN" --rpc)

LIFT1=$(printf '%s\n' "$LIFT_RESPONSES" | sed -n '1p')
LIFT2=$(printf '%s\n' "$LIFT_RESPONSES" | sed -n '2p')
LIFT3=$(printf '%s\n' "$LIFT_RESPONSES" | sed -n '3p')

# T14: lift initialize response contains protocol_version
check "T14 lift/initialize: protocol_version" "$LIFT1" '"protocol_version":"provekit-lift/1"'

# T15: lift response is ir-document
check "T15 lift: kind ir-document" "$LIFT2" '"kind":"ir-document"'

# T16: lift response contains ir array
check "T16 lift: ir key present" "$LIFT2" '"ir":'

# T17: lift response ir array contains contract 'add'
check "T17 lift: ir contains add" "$LIFT2" '"name":"add"'

# T18: lift response ir array contains contract 'compute'
check "T18 lift: ir contains compute" "$LIFT2" '"name":"compute"'

# T19: lift response callEdges is empty array
check "T19 lift: callEdges empty" "$LIFT2" '"callEdges":[]'

# T20: lift response contains diagnostics array
check "T20 lift: diagnostics key present" "$LIFT2" '"diagnostics":'

# T21: lift response contains opacityReport array
check "T21 lift: opacityReport key present" "$LIFT2" '"opacityReport":'

# T22: lift response contains refusals array
check "T22 lift: refusals key present" "$LIFT2" '"refusals":'

# T23: shutdown response contains null result
check "T23 lift/shutdown: result null" "$LIFT3" '"result":null'

printf "\nResults: %d passed, %d failed\n" "$PASS" "$FAIL"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
