#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-kernel-doc"
FIXTURE="$SCRIPT_DIR/fixtures/kernel_doc_basic.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$FIXTURE" | tr -d '\n' | sed 's/\\n$//')

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":17,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":42,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["kernel_doc_basic.c"],"surface":"c-kernel-doc"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":77,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

printf '%s\n' "$RESPONSES" | grep -q '"id":17' || {
    echo "FAIL: initialize did not echo id 17" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-kernel-doc"' || {
    echo "FAIL: initialize missing c-kernel-doc name" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"protocol_version":"provekit-lift/1"' || {
    echo "FAIL: initialize missing protocol version" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"id":42' || {
    echo "FAIL: lift did not echo id 42" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"kind":"ir-document"' || {
    echo "FAIL: lift missing ir-document kind" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-kernel-doc.context.must-hold"' || {
    echo "FAIL: lift missing Context must-hold contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-kernel-doc.return.negative-errno"' || {
    echo "FAIL: lift missing Return negative errno contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-kernel-doc.param.nonnull"' || {
    echo "FAIL: lift missing parameter nonnull contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"buf"' || {
    echo "FAIL: lift missing buf parameter binding" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-kernel-doc.param.positive"' || {
    echo "FAIL: lift missing parameter positive contract" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"name":"len"' || {
    echo "FAIL: lift missing len parameter binding" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"diagnostics":\[\]' || {
    echo "FAIL: valid fixture should have empty diagnostics" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"opacityReport":\[\]' || {
    echo "FAIL: valid fixture should have empty opacity report" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"refusals":\[\]' || {
    echo "FAIL: valid fixture should have empty refusals" >&2
    echo "$RESPONSES" >&2
    exit 1
}

BAD_SURFACE_RESPONSES="$(
    {
        printf '{"jsonrpc":"2.0","id":43,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["kernel_doc_basic.c"],"surface":"c-sparse"}}\n'
    } | "$BIN" --rpc
)"

printf '%s\n' "$BAD_SURFACE_RESPONSES" | grep -q '"message":"unsupported surface"' || {
    echo "FAIL: c-kernel-doc should reject other C-family surfaces" >&2
    echo "$BAD_SURFACE_RESPONSES" >&2
    exit 1
}

REQUEST="{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"parse\",\"params\":{\"path\":\"kernel_doc_basic.c\",\"source\":\"$SOURCE\"}}"
RESPONSE=$(printf '%s\n' "$REQUEST" | "$BIN" --rpc)

printf '%s\n' "$RESPONSE" | grep -q '"id":99' || {
    echo "FAIL: parse did not echo id 99" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSE" | grep -q '"name":"c-kernel-doc.context.must-hold"' || {
    echo "FAIL: parse missing Context must-hold contract" >&2
    echo "$RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSE" | grep -q '"name":"io_lock"' || {
    echo "FAIL: parse missing context lock binding" >&2
    echo "$RESPONSE" >&2
    exit 1
}

AST_REQUEST="{\"jsonrpc\":\"2.0\",\"id\":98,\"method\":\"parse\",\"params\":{\"path\":\"kernel_doc_basic.c\",\"parse_backend\":\"clang_ast\",\"source\":\"$SOURCE\"}}"
AST_RESPONSE=$(printf '%s\n' "$AST_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$AST_RESPONSE" | grep -q '"name":"c-kernel-doc.return.negative-errno"' || {
    echo "FAIL: clang_ast parse should preserve kernel-doc return contract" >&2
    echo "$AST_RESPONSE" >&2
    exit 1
}

KERNEL_CONTEXT_REQUEST="$(
    printf '{"jsonrpc":"2.0","id":97,"method":"parse","params":{"workspace_root":'
    printf '"%s"' "$SCRIPT_DIR/fixtures"
    printf ',"path":"kernel/missing.c","compile_context":"kernel","source":"%s"}}' "$SOURCE"
)"
KERNEL_CONTEXT_RESPONSE=$(printf '%s\n' "$KERNEL_CONTEXT_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$KERNEL_CONTEXT_RESPONSE" | grep -q '"kind":"kernel-compile-context-missing"' || {
    echo "FAIL: kernel compile context resolver opacity should flow through kernel-doc RPC" >&2
    echo "$KERNEL_CONTEXT_RESPONSE" >&2
    exit 1
}

MALFORMED_REQUEST='{"jsonrpc":"2.0","id":100,"method":"parse","params":{"path":"malformed.c","source":"/**\n * orphan - malformed kernel doc\n * @p: Must not be NULL.\n */\nint unrelated;\n"}}'
MALFORMED_RESPONSE=$(printf '%s\n' "$MALFORMED_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$MALFORMED_RESPONSE" | grep -q '"kind":"c-kernel-doc.unattached-comment"' || {
    echo "FAIL: unattached kernel-doc should emit diagnostic" >&2
    echo "$MALFORMED_RESPONSE" >&2
    exit 1
}

OPAQUE_REQUEST='{"jsonrpc":"2.0","id":101,"method":"parse","params":{"path":"conditional.c","source":"/**\n * maybe_run - conditional attachment\n * Return: 0 on success or negative errno on failure.\n */\n#ifdef CONFIG_FOO\nint maybe_run(void) { return 0; }\n#endif\n"}}'
OPAQUE_RESPONSE=$(printf '%s\n' "$OPAQUE_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$OPAQUE_RESPONSE" | grep -q '"kind":"c-kernel-doc.conditional-attachment"' || {
    echo "FAIL: preprocessor-separated kernel-doc should emit attachment opacity" >&2
    echo "$OPAQUE_RESPONSE" >&2
    exit 1
}

REFUSAL_REQUEST='{"jsonrpc":"2.0","id":102,"method":"parse","params":{"path":"ownership.c","source":"/**\n * acquire_ref - get a reference\n * Return: caller owns a reference and must release it.\n */\nvoid *acquire_ref(void) { return 0; }\n"}}'
REFUSAL_RESPONSE=$(printf '%s\n' "$REFUSAL_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$REFUSAL_RESPONSE" | grep -q '"kind":"c-kernel-doc.unsupported-return-ownership"' || {
    echo "FAIL: ownership return language should emit refusal" >&2
    echo "$REFUSAL_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$REFUSAL_RESPONSE" | grep -q '"refusals":\[' || {
    echo "FAIL: ownership refusal should stay in refusals stream" >&2
    echo "$REFUSAL_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"id":77' || {
    echo "FAIL: shutdown did not echo id 77" >&2
    echo "$RESPONSES" >&2
    exit 1
}

STRUCTURAL_FIXTURE="$SCRIPT_DIR/fixtures/structural_basic.c"
if [ ! -f "$STRUCTURAL_FIXTURE" ]; then
    echo "FAIL: structural fixture not found: $STRUCTURAL_FIXTURE" >&2
    exit 1
fi
STRUCTURAL_SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$STRUCTURAL_FIXTURE" | tr -d '\n' | sed 's/\\n$//')
STRUCTURAL_REQUEST="{\"jsonrpc\":\"2.0\",\"id\":110,\"method\":\"parse\",\"params\":{\"path\":\"structural_basic.c\",\"parse_backend\":\"clang_ast\",\"source\":\"$STRUCTURAL_SOURCE\"}}"
STRUCTURAL_RESPONSE=$(printf '%s\n' "$STRUCTURAL_REQUEST" | "$BIN" --rpc)

printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -q '"id":110' || {
    echo "FAIL: structural parse did not echo id 110" >&2
    echo "$STRUCTURAL_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -qE '"callEdges":\[\s*\{' || {
    echo "FAIL: clang_ast backend should surface call edges (callEdges array empty on file with callsites)" >&2
    echo "$STRUCTURAL_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -q '"callee_name":"helper_inplace"' || {
    echo "FAIL: callEdges should include helper_inplace callee per pinned schema (caller_function/callee_name/args_json/callsite_*)" >&2
    echo "$STRUCTURAL_RESPONSE" >&2
    exit 1
}

printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -q '"caller_function":"caller_unsafe"' || {
    echo "FAIL: callEdges should include caller_unsafe as a caller per pinned schema" >&2
    echo "$STRUCTURAL_RESPONSE" >&2
    exit 1
}

# The arg-extraction assertions below require the libclang AST backend.
# On builds without libclang, the stub emits an "ast-backend-unavailable"
# opacity entry, callEdges still surface via the regex backend (with empty
# args), and the args-specific assertions cannot be satisfied. Skip them
# in that branch so `make test` is meaningful but not red on stub builds.
if printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -q '"kind":"ast-backend-unavailable"'; then
    echo "info: clang_ast backend unavailable in this build; skipping arg-extraction assertions"
else
    printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -qE '"args":\[\{' || {
        echo "FAIL: callEdges should emit args as a direct JSON array of arg objects (not a quoted string)" >&2
        echo "$STRUCTURAL_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -q '"text":"external"' || {
        echo "FAIL: args extraction should surface the var text 'external' for caller_unsafe's helper_inplace call" >&2
        echo "$STRUCTURAL_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -q '"position":0' || {
        echo "FAIL: args entries should carry a position field per the pinned schema" >&2
        echo "$STRUCTURAL_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$STRUCTURAL_RESPONSE" | grep -qE '"kind":"(var|literal|expr)"' || {
        echo "FAIL: args entries should carry a kind classification (var|literal|expr)" >&2
        echo "$STRUCTURAL_RESPONSE" >&2
        exit 1
    }
fi

echo "provekit-lift-c-kernel-doc integration passed"
