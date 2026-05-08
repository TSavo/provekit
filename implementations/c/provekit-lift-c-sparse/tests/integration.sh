#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-sparse"
FIXTURE="$SCRIPT_DIR/fixtures/sparse_basic.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$FIXTURE" | tr -d '\n' | sed 's/\\n$//')
REQUEST="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"parse\",\"params\":{\"path\":\"sparse_basic.c\",\"source\":\"$SOURCE\"}}"
RESPONSE=$(printf '%s\n' "$REQUEST" | "$BIN" --rpc)

printf '%s\n' "$RESPONSE" | grep -q '"name":"c-sparse.user-pointer"' || {
    echo "FAIL: missing __user contract" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSE" | grep -q '"name":"c-sparse.must-hold"' || {
    echo "FAIL: missing __must_hold contract" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSE" | grep -q '"opacityReport":\[\]' || {
    echo "FAIL: sparse fixture should have empty opacity report" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf 'provekit-lift-c-sparse integration passed\n'
