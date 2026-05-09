#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/../.."
FIXTURE="$SCRIPT_DIR/fixtures/sparse_and_assertions.c"
SPARSE="$ROOT/provekit-lift-c-sparse/provekit-lift-c-sparse"
ASSERTIONS="$ROOT/provekit-lift-c-assertions/provekit-lift-c-assertions"

if [ ! -x "$SPARSE" ]; then
    echo "FAIL: sparse lifter binary not found: $SPARSE" >&2
    exit 1
fi

if [ ! -x "$ASSERTIONS" ]; then
    echo "FAIL: assertions lifter binary not found: $ASSERTIONS" >&2
    exit 1
fi

SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$FIXTURE" | tr -d '\n' | sed 's/\\n$//')
REQUEST="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"parse\",\"params\":{\"path\":\"sparse_and_assertions.c\",\"source\":\"$SOURCE\"}}"

SPARSE_RESPONSE=$(printf '%s\n' "$REQUEST" | "$SPARSE" --rpc)
ASSERTIONS_RESPONSE=$(printf '%s\n' "$REQUEST" | "$ASSERTIONS" --rpc)

printf '%s\n' "$SPARSE_RESPONSE" | grep -q '"id":1' || {
    echo "FAIL: sparse lifter did not echo id 1" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$ASSERTIONS_RESPONSE" | grep -q '"id":1' || {
    echo "FAIL: assertions lifter did not echo id 1" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$SPARSE_RESPONSE" | grep -q '"name":"c-sparse.user-pointer"' || {
    echo "FAIL: sparse lifter did not find __user contract" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$ASSERTIONS_RESPONSE" | grep -q '"name":"c-assertions.warn-on"' || {
    echo "FAIL: assertions lifter did not find WARN_ON contract" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
}

if printf '%s\n' "$SPARSE_RESPONSE" | grep -q '"name":"c-assertions.'; then
    echo "FAIL: sparse lifter emitted assertions contract" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
fi

if printf '%s\n' "$ASSERTIONS_RESPONSE" | grep -q '"name":"c-sparse.'; then
    echo "FAIL: assertions lifter emitted sparse contract" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
fi

printf '%s\n' "$SPARSE_RESPONSE" | grep -q '"refusals":\[\]' || {
    echo "FAIL: sparse lifter should have empty refusals" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$ASSERTIONS_RESPONSE" | grep -q '"opacityReport":\[\]' || {
    echo "FAIL: assertions lifter should have empty opacity report" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
}

printf 'C lifter composition integration passed\n'
