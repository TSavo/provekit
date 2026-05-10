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

contains_json() {
    printf '%s\n' "$1" | grep -Eq "$2"
}

contains_json "$SPARSE_RESPONSE" '"id"[[:space:]]*:[[:space:]]*1' || {
    echo "FAIL: sparse lifter did not echo id 1" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
}

contains_json "$ASSERTIONS_RESPONSE" '"id"[[:space:]]*:[[:space:]]*1' || {
    echo "FAIL: assertions lifter did not echo id 1" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
}

contains_json "$SPARSE_RESPONSE" '"name"[[:space:]]*:[[:space:]]*"c-sparse\.user-pointer"' || {
    echo "FAIL: sparse lifter did not find __user contract" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
}

contains_json "$ASSERTIONS_RESPONSE" '"kind"[[:space:]]*:[[:space:]]*"c-assertions\.warn-on"' || {
    echo "FAIL: assertions lifter did not report WARN_ON opacity" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
}

if contains_json "$ASSERTIONS_RESPONSE" '"name"[[:space:]]*:[[:space:]]*"c-assertions\.warn-on"'; then
    echo "FAIL: assertions lifter emitted WARN_ON as a hard contract" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
fi

if contains_json "$SPARSE_RESPONSE" '"name"[[:space:]]*:[[:space:]]*"c-assertions\.'; then
    echo "FAIL: sparse lifter emitted assertions contract" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
fi

if contains_json "$ASSERTIONS_RESPONSE" '"name"[[:space:]]*:[[:space:]]*"c-sparse\.'; then
    echo "FAIL: assertions lifter emitted sparse contract" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
fi

contains_json "$SPARSE_RESPONSE" '"refusals"[[:space:]]*:[[:space:]]*\[[[:space:]]*\]' || {
    echo "FAIL: sparse lifter should have empty refusals" >&2
    echo "$SPARSE_RESPONSE" >&2
    exit 1
}

contains_json "$ASSERTIONS_RESPONSE" '"refusals"[[:space:]]*:[[:space:]]*\[[[:space:]]*\]' || {
    echo "FAIL: assertions lifter should have empty refusals" >&2
    echo "$ASSERTIONS_RESPONSE" >&2
    exit 1
}

printf 'C lifter composition integration passed\n'
