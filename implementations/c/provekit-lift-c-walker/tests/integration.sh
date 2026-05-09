#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-walker"
FIXTURE="$SCRIPT_DIR/fixtures/trivial.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

if [ ! -f "$FIXTURE" ]; then
    echo "FAIL: fixture not found: $FIXTURE" >&2
    exit 1
fi

SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$FIXTURE" | tr -d '\n' | sed 's/\\n$//')

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["trivial.c"],"surface":"c-walker"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

printf '%s\n' "$RESPONSES" | grep -q '"id":1' || {
    echo "FAIL: initialize did not echo id 1" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-walker"' || {
    echo "FAIL: initialize missing c-walker name" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"protocol_version":"provekit-lift/1"' || {
    echo "FAIL: initialize missing protocol version" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"id":2' || {
    echo "FAIL: lift did not echo id 2" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"kind":"ir-document"' || {
    echo "FAIL: lift missing ir-document kind" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# declarations[] must be non-empty (at least one function-contract per function with body)
printf '%s\n' "$RESPONSES" | grep -q '"declarations":\[{' || {
    echo "FAIL: declarations must be non-empty" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# Each synthesized contract must have kind: function-contract
printf '%s\n' "$RESPONSES" | grep -q '"kind":"function-contract"' || {
    echo "FAIL: declarations must contain function-contract entries" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# fn_name field must be present
printf '%s\n' "$RESPONSES" | grep -q '"fn_name"' || {
    echo "FAIL: function-contract entries must have fn_name field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# pre must be present (trivial true predicate)
printf '%s\n' "$RESPONSES" | grep -q '"pre"' || {
    echo "FAIL: function-contract entries must have pre field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# post must be present
printf '%s\n' "$RESPONSES" | grep -q '"post"' || {
    echo "FAIL: function-contract entries must have post field" >&2
    echo "$RESPONSES" >&2
    exit 1
}

# Verify known function names are lifted (regex backend always finds them)
printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"add"' || {
    echo "FAIL: add function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"identity"' || {
    echo "FAIL: identity function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"negate"' || {
    echo "FAIL: negate function not found in declarations" >&2
    echo "$RESPONSES" >&2
    exit 1
}

echo "provekit-lift-c-walker integration passed"
