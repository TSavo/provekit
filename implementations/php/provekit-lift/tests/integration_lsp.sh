#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Integration test for provekit-lsp-php (lspd.php).
#
# Tests provekit-lsp-shared/1 protocol: initialize, analyzeDocument, lift,
# parse (legacy), shutdown.

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
check "T2 initialize: protocol_version"  "$LIFT1" '"protocol_version":"provekit-lsp-shared/1"'
check "T3 initialize: kit_id" "$LIFT1" '"kit_id":"php"'
check "T4 initialize: source_surfaces" "$LIFT1" '"source_surfaces":["php-source"]'
check "T4b initialize: diagnostic code" "$LIFT1" '"provekit.lsp.implication_failed"'
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

# ---------------------------------------------------------------------------
# Round 3: shared analyzeDocument method
# ---------------------------------------------------------------------------
printf "\n-- analyzeDocument --\n"

cat > "$TMPDIR_PATH/floor.php" << 'EOF'
<?php
// @provekit-contract
function checkPositive(int $x): bool {
    if ($x <= 0) { return false; }
    return true;
}
// @provekit-contract
function callerSatisfiesPre(): bool {
    $result = checkPositive(5);
    return $result;
}
// @provekit-contract
function callerViolatesPre(): bool {
    $result = checkPositive(-1);
    return $result;
}
function callerWithLoop(): bool {
    for ($i = 0; $i < 10; $i++) {
        $result = checkPositive($i);
        if (!$result) { return false; }
    }
    return true;
}
EOF

FLOOR_SOURCE=$(cat "$TMPDIR_PATH/floor.php" | sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' | tr -d '\n' | sed 's/\\n$//')
ANALYZE_REQUESTS=$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":20,"method":"initialize","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":21,\"method\":\"analyzeDocument\",\"params\":{\"kit_id\":\"php\",\"uri\":\"file:///project/floor.php\",\"file\":\"floor.php\",\"text\":\"$FLOOR_SOURCE\",\"document_version\":42,\"workspace_root\":\"/project\",\"accepted_protocol_catalog_cids\":[],\"policy_cids\":[]}}" \
    '{"jsonrpc":"2.0","id":22,"method":"shutdown"}')

ANALYZE_RESPONSES=$(printf '%s\n' "$ANALYZE_REQUESTS" | "$PHP_BIN" "$LSPD")
ANALYZE2=$(printf '%s\n' "$ANALYZE_RESPONSES" | sed -n '2p')

check "T19 analyze: kind" "$ANALYZE2" '"kind":"lsp-document-analysis"'
check "T20 analyze: schema" "$ANALYZE2" '"schema_version":"1"'
check "T21 analyze: kit" "$ANALYZE2" '"kit_id":"php"'
check "T22 analyze: document cid" "$ANALYZE2" '"document_cid":"blake3-512:'
check "T23 analyze: statuses empty" "$ANALYZE2" '"statuses":[]'
check "T24 analyze: project null" "$ANALYZE2" '"project":null'
check "T25 analyze: diagnostic code" "$ANALYZE2" '"code":"provekit.lsp.implication_failed"'
check "T26 analyze: diagnostic producer" "$ANALYZE2" '"producer":"forward-propagation"'
check "T27 analyze: range line" "$ANALYZE2" '"start_line":14'
check "T28 analyze: range column" "$ANALYZE2" '"start_col":14'
check "T29 analyze: callee" "$ANALYZE2" '"callee":"\\checkPositive"'

# Cleanup
rm -rf "$TMPDIR_PATH"

printf "\nResults: %d passed, %d failed\n" "$PASS" "$FAIL"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
