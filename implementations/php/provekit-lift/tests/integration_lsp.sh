#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Integration test for provekit-lsp-php (lspd.php).
#
# Tests provekit-lift/1 protocol: initialize, lift, parse (legacy), shutdown.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LSPD="$SCRIPT_DIR/../src/lspd.php"

if [ ! -f "$LSPD" ]; then
    echo "FAIL: lspd.php not found: $LSPD" >&2
    exit 1
fi

PHP_BIN="${PHP_BIN:-php}"

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
# Create a temp fixture PHP file with a @provekit-contract annotation.
# ---------------------------------------------------------------------------
TMPDIR_PATH="$(mktemp -d)"
FIXTURE="$TMPDIR_PATH/fixture.php"
cat > "$FIXTURE" << 'EOF'
<?php
// @provekit-contract
function add_numbers(int $a, int $b): int {
    return $a + $b;
}
// @provekit-contract
function compute_total(int $a, int $b): int {
    return add_numbers($a, $b);
}
EOF

printf "Running provekit-lsp-php integration tests...\n"

# ---------------------------------------------------------------------------
# Round 1: initialize + lift + shutdown
# ---------------------------------------------------------------------------
printf "\n-- initialize / lift / shutdown --\n"

LIFT_REQUESTS=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"lift\",\"params\":{\"workspace_root\":\"$TMPDIR_PATH\",\"source_paths\":[\"fixture.php\"]}}" \
    '{"jsonrpc":"2.0","id":3,"method":"shutdown"}')

LIFT_RESPONSES=$(printf '%s\n' "$LIFT_REQUESTS" | "$PHP_BIN" "$LSPD")

LIFT1=$(printf '%s\n' "$LIFT_RESPONSES" | sed -n '1p')
LIFT2=$(printf '%s\n' "$LIFT_RESPONSES" | sed -n '2p')
LIFT3=$(printf '%s\n' "$LIFT_RESPONSES" | sed -n '3p')

check "T1 initialize: name"              "$LIFT1" '"name":"provekit-lsp-php"'
check "T2 initialize: protocol_version"  "$LIFT1" '"protocol_version":"provekit-lift'
check "T3 initialize: authoring_surfaces" "$LIFT1" '"authoring_surfaces":["php-source"]'
check "T4 initialize: emits_signed_mementos false" "$LIFT1" '"emits_signed_mementos":false'
check "T5 lift: kind ir-document"        "$LIFT2" '"kind":"ir-document"'
check "T6 lift: ir key present"          "$LIFT2" '"ir":'
check "T7 lift: contract add_numbers"    "$LIFT2" '"name":"add_numbers"'
check "T8 lift: callEdges empty"         "$LIFT2" '"callEdges":[]'
check "T9 lift: diagnostics key present" "$LIFT2" '"diagnostics":'
check "T10 lift: refusals key present"   "$LIFT2" '"refusals":'
check "T11 shutdown: result null"        "$LIFT3" '"result":null'

# ---------------------------------------------------------------------------
# Round 2: legacy parse method
# ---------------------------------------------------------------------------
printf "\n-- legacy parse --\n"

SOURCE=$(cat "$FIXTURE" | sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' | tr -d '\n' | sed 's/\\n$//')

PARSE_REQUESTS=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":10,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"parse\",\"params\":{\"path\":\"fixture.php\",\"source\":\"$SOURCE\"}}" \
    '{"jsonrpc":"2.0","id":12,"method":"shutdown"}')

PARSE_RESPONSES=$(printf '%s\n' "$PARSE_REQUESTS" | "$PHP_BIN" "$LSPD")

PARSE2=$(printf '%s\n' "$PARSE_RESPONSES" | sed -n '2p')

check "T12 parse: declarations key"    "$PARSE2" '"declarations":'
check "T13 parse: contract add_numbers" "$PARSE2" '"name":"add_numbers"'
check "T14 parse: call edge source"    "$PARSE2" '"sourceContractCid":"pending-php:compute_total"'
check "T15 parse: call edge target"    "$PARSE2" '"targetSymbol":"php-kit:add_numbers"'
check "T16 parse: call edge locus file" "$PARSE2" '"file":"fixture.php"'
check "T17 parse: call edge locus line" "$PARSE2" '"line":8'
check "T18 parse: call edge locus column" "$PARSE2" '"column":11'

# Cleanup
rm -rf "$TMPDIR_PATH"

printf "\nResults: %d passed, %d failed\n" "$PASS" "$FAIL"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
