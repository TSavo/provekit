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

printf '%s\n' "$RESPONSES" | grep -q '"callEdges":\[\]' || {
    echo "FAIL: lift missing callEdges stream" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"opacityReport":\[\]' || {
    echo "FAIL: lift missing opacityReport stream" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"refusals":\[\]' || {
    echo "FAIL: lift missing refusals stream" >&2
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

DEFINE_ONLY_REQUEST='{"jsonrpc":"2.0","id":100,"method":"parse","params":{"path":"define_only.c","source":"#define __user\n"}}'
DEFINE_ONLY_RESPONSE=$(printf '%s\n' "$DEFINE_ONLY_REQUEST" | "$BIN" --rpc)

if printf '%s\n' "$DEFINE_ONLY_RESPONSE" | grep -q '"name":"c-sparse.user-pointer"'; then
    echo "FAIL: __user define should not emit user-pointer contract" >&2
    echo "$DEFINE_ONLY_RESPONSE" >&2
    exit 1
fi

COMMENT_ONLY_REQUEST='{"jsonrpc":"2.0","id":101,"method":"parse","params":{"path":"comment_only.c","source":"int quiet(void) { /* __must_hold(lock) */ return 0; }\n"}}'
COMMENT_ONLY_RESPONSE=$(printf '%s\n' "$COMMENT_ONLY_REQUEST" | "$BIN" --rpc)

if printf '%s\n' "$COMMENT_ONLY_RESPONSE" | grep -q '"name":"c-sparse.must-hold"'; then
    echo "FAIL: sparse annotation in comment should not emit contract" >&2
    echo "$COMMENT_ONLY_RESPONSE" >&2
    exit 1
fi

FIELD_COLLISION_REQUEST='{"jsonrpc":"2.0","id":102,"method":"parse","params":{"path":"source","source":"int copy_name(char __user *buf) { return 0; }\n"}}'
FIELD_COLLISION_RESPONSE=$(printf '%s\n' "$FIELD_COLLISION_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$FIELD_COLLISION_RESPONSE" | grep -q '"id":102' || {
    echo "FAIL: parse with source-valued path did not echo id 102" >&2
    echo "$FIELD_COLLISION_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$FIELD_COLLISION_RESPONSE" | grep -q '"name":"c-sparse.user-pointer"' || {
    echo "FAIL: parse with source-valued path missed __user contract" >&2
    echo "$FIELD_COLLISION_RESPONSE" >&2
    exit 1
}

NESTED_METHOD_REQUEST='{"jsonrpc":"2.0","id":103,"params":{"method":"shutdown","path":"nested_method.c","source":"int copy_name(char __user *buf) { return 0; }\n"},"method":"parse"}'
NESTED_METHOD_RESPONSE=$(printf '%s\n' "$NESTED_METHOD_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$NESTED_METHOD_RESPONSE" | grep -q '"id":103' || {
    echo "FAIL: nested params.method should not override top-level id" >&2
    echo "$NESTED_METHOD_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$NESTED_METHOD_RESPONSE" | grep -q '"name":"c-sparse.user-pointer"' || {
    echo "FAIL: nested params.method should not override top-level parse method" >&2
    echo "$NESTED_METHOD_RESPONSE" >&2
    exit 1
}

REPEATED_LOCKS_REQUEST='{"jsonrpc":"2.0","id":104,"method":"parse","params":{"path":"locks.c","source":"void a(int *p) __must_hold(lock_a) { }\nvoid b(int *p) __must_hold(lock_b) { }\n"}}'
REPEATED_LOCKS_RESPONSE=$(printf '%s\n' "$REPEATED_LOCKS_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$REPEATED_LOCKS_RESPONSE" | grep -q '"name":"lock_a"' || {
    echo "FAIL: repeated __must_hold should emit first lock" >&2
    echo "$REPEATED_LOCKS_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$REPEATED_LOCKS_RESPONSE" | grep -q '"name":"lock_b"' || {
    echo "FAIL: repeated __must_hold should emit second lock" >&2
    echo "$REPEATED_LOCKS_RESPONSE" >&2
    exit 1
}

ESCAPED_LOCK_REQUEST='{"jsonrpc":"2.0","id":105,"method":"parse","params":{"path":"escaped.c","source":"void a(int *p) __must_hold(lock\\quoted) { }\n"}}'
ESCAPED_LOCK_RESPONSE=$(printf '%s\n' "$ESCAPED_LOCK_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$ESCAPED_LOCK_RESPONSE" | grep -qF 'lock\\quoted' || {
    echo "FAIL: sparse annotation backslash argument should be JSON-escaped" >&2
    echo "$ESCAPED_LOCK_RESPONSE" >&2
    exit 1
}

UNICODE_LOCK_REQUEST='{"jsonrpc":"2.0","id":106,"method":"parse","params":{"path":"unicode.c","source":"void a(int *p) __must_hold(lock\u005fa) { }\n"}}'
UNICODE_LOCK_RESPONSE=$(printf '%s\n' "$UNICODE_LOCK_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$UNICODE_LOCK_RESPONSE" | grep -q '"name":"lock_a"' || {
    echo "FAIL: JSON unicode escapes should decode before sparse lifting" >&2
    echo "$UNICODE_LOCK_RESPONSE" >&2
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

assert_invalid_lift_params() {
    name="$1"
    response="$2"

    printf '%s\n' "$response" | grep -q '"error"' || {
        echo "FAIL: $name should return an error" >&2
        echo "$response" >&2
        exit 1
    }

    printf '%s\n' "$response" | grep -q '"code":-32602' || {
        echo "FAIL: $name should return invalid params -32602" >&2
        echo "$response" >&2
        exit 1
    }

    if printf '%s\n' "$response" | grep -q '"kind":"ir-document"'; then
        echo "FAIL: $name should not return an ir-document" >&2
        echo "$response" >&2
        exit 1
    fi
}

MISSING_SOURCE_PATHS="$(
    {
        printf '{"jsonrpc":"2.0","id":89,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"surface":"c-sparse"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "missing source_paths" "$MISSING_SOURCE_PATHS"

EMPTY_SOURCE_PATHS="$(
    {
        printf '{"jsonrpc":"2.0","id":90,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":[],"surface":"c-sparse"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "empty source_paths" "$EMPTY_SOURCE_PATHS"

EMPTY_SOURCE_PATH="$(
    {
        printf '{"jsonrpc":"2.0","id":91,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":[""],"surface":"c-sparse"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "empty source path" "$EMPTY_SOURCE_PATH"

MISSING_SURFACE="$(
    {
        printf '{"jsonrpc":"2.0","id":92,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["sparse_basic.c"]}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "missing surface" "$MISSING_SURFACE"

UNSUPPORTED_SURFACE="$(
    {
        printf '{"jsonrpc":"2.0","id":93,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["sparse_basic.c"],"surface":"c-assertions"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "unsupported surface" "$UNSUPPORTED_SURFACE"

printf 'provekit-lift-c-sparse integration passed\n'
