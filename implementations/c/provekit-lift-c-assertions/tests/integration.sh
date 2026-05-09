#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-assertions"
FIXTURE="$SCRIPT_DIR/fixtures/assertions_basic.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":17,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":42,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["assertions_basic.c"],"surface":"c-assertions"}}\n'
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

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-assertions"' || {
    echo "FAIL: initialize missing c-assertions name" >&2
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

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-assertions.warn-on"' || {
    echo "FAIL: lift missing WARN_ON contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-assertions.build-bug-on"' || {
    echo "FAIL: lift missing BUILD_BUG_ON contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

if printf '%s\n' "$RESPONSES" | grep -q '"name":"c-assertions.bug-on"'; then
    echo "FAIL: BUILD_BUG_ON should not emit BUG_ON contract" >&2
    echo "$RESPONSES" >&2
    exit 1
fi

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
REQUEST="{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"parse\",\"params\":{\"path\":\"assertions_basic.c\",\"source\":\"$SOURCE\"}}"
RESPONSE=$(printf '%s\n' "$REQUEST" | "$BIN" --rpc)

printf '%s\n' "$RESPONSE" | grep -q '"id":99' || {
    echo "FAIL: parse did not echo id 99" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSE" | grep -q '"name":"c-assertions.warn-on"' || {
    echo "FAIL: parse missing WARN_ON contract" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSE" | grep -q '"name":"c-assertions.build-bug-on"' || {
    echo "FAIL: parse missing BUILD_BUG_ON contract" >&2
    echo "$RESPONSE" >&2
    exit 1
}

if printf '%s\n' "$RESPONSE" | grep -q '"name":"c-assertions.bug-on"'; then
    echo "FAIL: BUILD_BUG_ON parse should not emit BUG_ON contract" >&2
    echo "$RESPONSE" >&2
    exit 1
fi

printf '%s\n' "$RESPONSE" | grep -q '"opacityReport":\[\]' || {
    echo "FAIL: assertions fixture should have empty opacity report" >&2
    echo "$RESPONSE" >&2
    exit 1
}

AST_ASSERT_REQUEST='{"jsonrpc":"2.0","id":98,"method":"parse","params":{"path":"ast_assert.c","parse_backend":"clang_ast","source":"void assert(int);\nvoid check(int bad) { (assert)(bad); }\n"}}'
AST_ASSERT_RESPONSE=$(printf '%s\n' "$AST_ASSERT_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$AST_ASSERT_RESPONSE" | grep -q '"id":98' || {
    echo "FAIL: AST assertions parse did not echo id 98" >&2
    echo "$AST_ASSERT_RESPONSE" >&2
    exit 1
}

if ! printf '%s\n' "$AST_ASSERT_RESPONSE" | grep -q '"name":"c-assertions.assert"' &&
    ! printf '%s\n' "$AST_ASSERT_RESPONSE" | grep -q '"kind":"ast-backend-unavailable"'; then
    echo "FAIL: clang_ast parse should either emit AST-only assert contract or report AST opacity" >&2
    echo "$AST_ASSERT_RESPONSE" >&2
    exit 1
fi

KERNEL_CONTEXT_REQUEST="$(
    printf '{"jsonrpc":"2.0","id":97,"method":"parse","params":{"workspace_root":'
    printf '"%s"' "$SCRIPT_DIR/fixtures"
    printf ',"path":"kernel/missing.c","compile_context":"kernel","source":"void check(int bad) { WARN_ON(bad); }\\n"}}'
)"
KERNEL_CONTEXT_RESPONSE=$(printf '%s\n' "$KERNEL_CONTEXT_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$KERNEL_CONTEXT_RESPONSE" | grep -q '"kind":"kernel-compile-context-missing"' || {
    echo "FAIL: kernel compile context resolver opacity should flow through assertions RPC" >&2
    echo "$KERNEL_CONTEXT_RESPONSE" >&2
    exit 1
}

COMMENT_ONLY_REQUEST='{"jsonrpc":"2.0","id":100,"method":"parse","params":{"path":"comment_only.c","source":"/* WARN_ON(noise) */\nint quiet(void) { const char *s = \"BUG_ON(noise)\"; return 0; }\n"}}'
COMMENT_ONLY_RESPONSE=$(printf '%s\n' "$COMMENT_ONLY_REQUEST" | "$BIN" --rpc)

if printf '%s\n' "$COMMENT_ONLY_RESPONSE" | grep -q '"name":"c-assertions.warn-on"'; then
    echo "FAIL: WARN_ON in comment should not emit contract" >&2
    echo "$COMMENT_ONLY_RESPONSE" >&2
    exit 1
fi

if printf '%s\n' "$COMMENT_ONLY_RESPONSE" | grep -q '"name":"c-assertions.bug-on"'; then
    echo "FAIL: BUG_ON in string should not emit contract" >&2
    echo "$COMMENT_ONLY_RESPONSE" >&2
    exit 1
fi

NESTED_METHOD_REQUEST='{"jsonrpc":"2.0","id":103,"params":{"method":"shutdown","path":"nested_method.c","source":"void f(int bad) { WARN_ON(bad); }\n"},"method":"parse"}'
NESTED_METHOD_RESPONSE=$(printf '%s\n' "$NESTED_METHOD_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$NESTED_METHOD_RESPONSE" | grep -q '"id":103' || {
    echo "FAIL: nested params.method should not override top-level id" >&2
    echo "$NESTED_METHOD_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$NESTED_METHOD_RESPONSE" | grep -q '"name":"c-assertions.warn-on"' || {
    echo "FAIL: nested params.method should not override top-level parse method" >&2
    echo "$NESTED_METHOD_RESPONSE" >&2
    exit 1
}

BUG_ON_REQUEST='{"jsonrpc":"2.0","id":101,"method":"parse","params":{"path":"bug_on.c","source":"void crash_if_bad(int bad) { BUG_ON(bad); }\n"}}'
BUG_ON_RESPONSE=$(printf '%s\n' "$BUG_ON_REQUEST" | "$BIN" --rpc)

if printf '%s\n' "$BUG_ON_RESPONSE" | grep -q '"name":"c-assertions.bug-on"'; then
    echo "FAIL: BUG_ON should not emit a positive contract declaration" >&2
    echo "$BUG_ON_RESPONSE" >&2
    exit 1
fi

printf '%s\n' "$BUG_ON_RESPONSE" | grep -q '"kind":"c-assertions.bug-on"' || {
    echo "FAIL: BUG_ON should be reported as a refusal" >&2
    echo "$BUG_ON_RESPONSE" >&2
    exit 1
}

BUG_ON_LIFT_RESPONSES="$(
    {
        printf '{"jsonrpc":"2.0","id":102,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["assertions_refusal.c"],"surface":"c-assertions"}}\n'
    } | "$BIN" --rpc
)"

if printf '%s\n' "$BUG_ON_LIFT_RESPONSES" | grep -q '"name":"c-assertions.bug-on"'; then
    echo "FAIL: BUG_ON lift should not emit a positive contract declaration" >&2
    echo "$BUG_ON_LIFT_RESPONSES" >&2
    exit 1
fi

printf '%s\n' "$BUG_ON_LIFT_RESPONSES" | grep -q '"kind":"c-assertions.bug-on"' || {
    echo "FAIL: BUG_ON lift should preserve refusal stream" >&2
    echo "$BUG_ON_LIFT_RESPONSES" >&2
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
        printf ',"source_paths":["assertions_basic.c",1],"surface":"c-assertions"}}\n'
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

printf '%s\n' "$MALFORMED_SOURCE_PATHS" | grep -q '"code":-32602' || {
    echo "FAIL: malformed source_paths should return invalid params -32602" >&2
    echo "$MALFORMED_SOURCE_PATHS" >&2
    exit 1
}

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
        printf ',"surface":"c-assertions"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "missing source_paths" "$MISSING_SOURCE_PATHS"

EMPTY_SOURCE_PATHS="$(
    {
        printf '{"jsonrpc":"2.0","id":90,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":[],"surface":"c-assertions"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "empty source_paths" "$EMPTY_SOURCE_PATHS"

EMPTY_SOURCE_PATH="$(
    {
        printf '{"jsonrpc":"2.0","id":91,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":[""],"surface":"c-assertions"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "empty source path" "$EMPTY_SOURCE_PATH"

MISSING_SURFACE="$(
    {
        printf '{"jsonrpc":"2.0","id":92,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["assertions_basic.c"]}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "missing surface" "$MISSING_SURFACE"

UNSUPPORTED_SURFACE="$(
    {
        printf '{"jsonrpc":"2.0","id":93,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["assertions_basic.c"],"surface":"c-sparse"}}\n'
    } | "$BIN" --rpc
)"
assert_invalid_lift_params "unsupported surface" "$UNSUPPORTED_SURFACE"

SYMLINK_ROOT="$(mktemp -d)"
trap 'rm -rf "$SYMLINK_ROOT"' EXIT
ln -s "$SCRIPT_DIR/fixtures" "$SYMLINK_ROOT/linked-fixtures"
SYMLINK_RESPONSE="$(
    {
        printf '{"jsonrpc":"2.0","id":94,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SYMLINK_ROOT"
        printf ',"source_paths":["linked-fixtures"],"surface":"c-assertions"}}\n'
    } | "$BIN" --rpc
)"

if printf '%s\n' "$SYMLINK_RESPONSE" | grep -q '"name":"c-assertions.'; then
    echo "FAIL: symlinked directories should not be traversed" >&2
    echo "$SYMLINK_RESPONSE" >&2
    exit 1
fi

if printf '%s\n' "$SYMLINK_RESPONSE" | grep -q '"kind":"c-assertions.bug-on"'; then
    echo "FAIL: symlinked directories should not preserve nested refusals" >&2
    echo "$SYMLINK_RESPONSE" >&2
    exit 1
fi

printf 'provekit-lift-c-assertions integration passed\n'
