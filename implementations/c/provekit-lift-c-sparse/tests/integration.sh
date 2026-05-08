#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-sparse"
FIXTURE="$SCRIPT_DIR/fixtures/sparse_basic.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":17,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":42,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["sparse_basic.c"],"surface":"c-sparse"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":77,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

printf '%s\n' "$RESPONSES" | grep -q '"id":17' || {
    echo "FAIL: initialize did not echo id 17" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"id":42' || {
    echo "FAIL: lift did not echo id 42" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"id":77' || {
    echo "FAIL: shutdown did not echo id 77" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-sparse"' || {
    echo "FAIL: initialize missing c-sparse name" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"protocol_version":"provekit-lift/1"' || {
    echo "FAIL: initialize missing protocol version" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"kind":"ir-document"' || {
    echo "FAIL: lift missing ir-document kind" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"ir":\[' || {
    echo "FAIL: lift missing ir array" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-sparse.user-pointer"' || {
    echo "FAIL: lift missing __user contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-sparse.must-hold"' || {
    echo "FAIL: lift missing __must_hold contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"diagnostics":\[\]' || {
    echo "FAIL: lift should have empty diagnostics" >&2
    echo "$RESPONSES" >&2
    exit 1
}

SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$FIXTURE" | tr -d '\n' | sed 's/\\n$//')
REQUEST="{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"parse\",\"params\":{\"path\":\"sparse_basic.c\",\"source\":\"$SOURCE\"}}"
RESPONSE=$(printf '%s\n' "$REQUEST" | "$BIN" --rpc)

printf '%s\n' "$RESPONSE" | grep -q '"id":99' || {
    echo "FAIL: parse did not echo id 99" >&2
    echo "$RESPONSE" >&2
    exit 1
}

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

MALFORMED_JSON_RESPONSE=$(printf '%s\n' '{"jsonrpc":"2.0","id":55,"method":"initialize"' | "$BIN" --rpc)

printf '%s\n' "$MALFORMED_JSON_RESPONSE" | grep -q '"error"' || {
    echo "FAIL: malformed JSON should return an error" >&2
    echo "$MALFORMED_JSON_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$MALFORMED_JSON_RESPONSE" | grep -q '"code":-32700' || {
    echo "FAIL: malformed JSON should return parse error -32700" >&2
    echo "$MALFORMED_JSON_RESPONSE" >&2
    exit 1
}

TRAILING_COMMA_RESPONSE=$(printf '%s\n' '{"jsonrpc":"2.0","id":56,"method":"initialize",}' | "$BIN" --rpc)

printf '%s\n' "$TRAILING_COMMA_RESPONSE" | grep -q '"error"' || {
    echo "FAIL: trailing comma JSON should return an error" >&2
    echo "$TRAILING_COMMA_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$TRAILING_COMMA_RESPONSE" | grep -q '"code":-32700' || {
    echo "FAIL: trailing comma JSON should return parse error -32700" >&2
    echo "$TRAILING_COMMA_RESPONSE" >&2
    exit 1
}

MISSING_COMMA_RESPONSE=$(printf '%s\n' '{"jsonrpc":"2.0","id":57,"method":"initialize" "params":{}}' | "$BIN" --rpc)

printf '%s\n' "$MISSING_COMMA_RESPONSE" | grep -q '"error"' || {
    echo "FAIL: missing comma JSON should return an error" >&2
    echo "$MISSING_COMMA_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$MISSING_COMMA_RESPONSE" | grep -q '"code":-32700' || {
    echo "FAIL: missing comma JSON should return parse error -32700" >&2
    echo "$MISSING_COMMA_RESPONSE" >&2
    exit 1
}

MALFORMED_SOURCE_PATHS="$(
    {
        printf '{"jsonrpc":"2.0","id":88,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["sparse_basic.c",1],"surface":"c-sparse"}}\n'
    } | "$BIN" --rpc
)"

printf '%s\n' "$MALFORMED_SOURCE_PATHS" | grep -q '"error"' || {
    echo "FAIL: malformed source_paths should return an error" >&2
    echo "$MALFORMED_SOURCE_PATHS" >&2
    exit 1
}

if printf '%s\n' "$MALFORMED_SOURCE_PATHS" | grep -q '"kind":"ir-document"'; then
    echo "FAIL: malformed source_paths should not return an ir-document" >&2
    echo "$MALFORMED_SOURCE_PATHS" >&2
    exit 1
fi

printf 'provekit-lift-c-sparse integration passed\n'
