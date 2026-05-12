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

printf '%s\n' "$RESPONSES" | grep -q '"protocol_version":"pep/1.7.0"' || {
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

    # Recovery-wrapped calls (RecoveryExpr / CXCursor_UnexposedExpr)
    # show up in real kernel C whenever an arg has a dependent type,
    # e.g. a local declared with an undeclared typedef. The lifter must
    # surface the call name even when libclang wraps the call instead
    # of producing a regular CallExpr cursor; otherwise the cluster
    # predicate misses critical sites such as aead_request_set_crypt
    # in net/ipv4/esp4.c.
    RECOVERY_FIXTURE="$SCRIPT_DIR/fixtures/recovery_call.c"
    if [ ! -f "$RECOVERY_FIXTURE" ]; then
        echo "FAIL: recovery fixture not found: $RECOVERY_FIXTURE" >&2
        exit 1
    fi
    RECOVERY_SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$RECOVERY_FIXTURE" | tr -d '\n' | sed 's/\\n$//')
    RECOVERY_REQUEST="{\"jsonrpc\":\"2.0\",\"id\":120,\"method\":\"parse\",\"params\":{\"path\":\"recovery_call.c\",\"parse_backend\":\"clang_ast\",\"source\":\"$RECOVERY_SOURCE\"}}"
    RECOVERY_RESPONSE=$(printf '%s\n' "$RECOVERY_REQUEST" | "$BIN" --rpc)

    CALLBACK_COUNT=$(printf '%s\n' "$RECOVERY_RESPONSE" | grep -o '"callee_name":"target_callback_set"' | wc -l | tr -d '[:space:]')
    if [ "$CALLBACK_COUNT" != "1" ]; then
        echo "FAIL: regular CallExpr target_callback_set should be lifted exactly once in recovery fixture (got $CALLBACK_COUNT)" >&2
        echo "$RECOVERY_RESPONSE" >&2
        exit 1
    fi

    INPLACE_COUNT=$(printf '%s\n' "$RECOVERY_RESPONSE" | grep -o '"callee_name":"target_inplace_set"' | wc -l | tr -d '[:space:]')
    if [ "$INPLACE_COUNT" != "1" ]; then
        echo "FAIL: RecoveryExpr-wrapped call target_inplace_set must be surfaced exactly once as a callEdge (got $INPLACE_COUNT)" >&2
        echo "$RECOVERY_RESPONSE" >&2
        exit 1
    fi

    NONCALL_REF_COUNT=$(printf '%s\n' "$RECOVERY_RESPONSE" | grep -o '"callee_name":"target_noncall_ref"' | wc -l | tr -d '[:space:]')
    if [ "$NONCALL_REF_COUNT" != "1" ]; then
        echo "FAIL: function references to target_noncall_ref must not be counted as calls; only the real call should surface (got $NONCALL_REF_COUNT)" >&2
        echo "$RECOVERY_RESPONSE" >&2
        exit 1
    fi

    PAREN_CALL_COUNT=$(printf '%s\n' "$RECOVERY_RESPONSE" | grep -o '"callee_name":"target_parenthesized_set"' | wc -l | tr -d '[:space:]')
    if [ "$PAREN_CALL_COUNT" != "1" ]; then
        echo "FAIL: parenthesized function designator target_parenthesized_set should be lifted exactly once (got $PAREN_CALL_COUNT)" >&2
        echo "$RECOVERY_RESPONSE" >&2
        exit 1
    fi

    DEREF_CALL_COUNT=$(printf '%s\n' "$RECOVERY_RESPONSE" | grep -o '"callee_name":"target_deref_set"' | wc -l | tr -d '[:space:]')
    if [ "$DEREF_CALL_COUNT" != "1" ]; then
        echo "FAIL: dereferenced function designator target_deref_set should be lifted exactly once (got $DEREF_CALL_COUNT)" >&2
        echo "$RECOVERY_RESPONSE" >&2
        exit 1
    fi

    # Per-function effects extraction per CCP v1.0.0 section 3.
    # Composition refuses on impure atoms; a lifter that emits no
    # effects is unsound. The fixture has one function per effect kind
    # plus pure_function with empty effects.
    EFFECTS_FIXTURE="$SCRIPT_DIR/fixtures/effects_basic.c"
    if [ ! -f "$EFFECTS_FIXTURE" ]; then
        echo "FAIL: effects fixture not found: $EFFECTS_FIXTURE" >&2
        exit 1
    fi
    EFFECTS_SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$EFFECTS_FIXTURE" | tr -d '\n' | sed 's/\\n$//')
    EFFECTS_REQUEST="{\"jsonrpc\":\"2.0\",\"id\":130,\"method\":\"parse\",\"params\":{\"path\":\"effects_basic.c\",\"parse_backend\":\"clang_ast\",\"source\":\"$EFFECTS_SOURCE\"}}"
    EFFECTS_RESPONSE=$(printf '%s\n' "$EFFECTS_REQUEST" | "$BIN" --rpc)

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -q '"id":130' || {
        echo "FAIL: effects parse did not echo id 130" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"pure_function","kind":"function-effects","effects":\[\]' || {
        echo "FAIL: pure_function should emit empty effects array" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"writes_function","kind":"function-effects","effects":\[[^]]*"kind":"Writes"' || {
        echo "FAIL: writes_function should emit Writes effect" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"reads_function","kind":"function-effects","effects":\[[^]]*"kind":"Reads"' || {
        echo "FAIL: reads_function should emit Reads effect" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"io_function","kind":"function-effects","effects":\[[^]]*"kind":"Io"' || {
        echo "FAIL: io_function should emit Io effect from kmalloc allowlist" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"unsafe_function","kind":"function-effects","effects":\[[^]]*"kind":"Unsafe"' || {
        echo "FAIL: unsafe_function should emit Unsafe effect from non-void pointer cast" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"panics_function","kind":"function-effects","effects":\[[^]]*"kind":"Panics"' || {
        echo "FAIL: panics_function should emit Panics effect from BUG_ON allowlist" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    printf '%s\n' "$EFFECTS_RESPONSE" | grep -qE '"function":"unresolved_call_function","kind":"function-effects","effects":\[[^]]*"kind":"UnresolvedCall"' || {
        echo "FAIL: unresolved_call_function should emit UnresolvedCall effect from struct-member function pointer dispatch" >&2
        echo "$EFFECTS_RESPONSE" >&2
        exit 1
    }

    # Composition pass per CCP v1.0.0 section 4 (eager materialization)
    # and section 6.2 (C ABI FFI). The composition_basic.c fixture has
    # three pure helpers chained via direct calls; the C lifter must
    # walk the call_sites graph, identify the pure subtree, JCS-encode
    # the chain, call libprovekit's pk_compose_chain_contracts via the
    # FFI, and emit the resulting ComposedFunctionContract back into
    # the IR document under kind="composed-contract".
    COMPOSITION_FIXTURE="$SCRIPT_DIR/fixtures/composition_basic.c"
    if [ ! -f "$COMPOSITION_FIXTURE" ]; then
        echo "FAIL: composition fixture not found: $COMPOSITION_FIXTURE" >&2
        exit 1
    fi
    COMPOSITION_SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$COMPOSITION_FIXTURE" | tr -d '\n' | sed 's/\\n$//')
    COMPOSITION_REQUEST="{\"jsonrpc\":\"2.0\",\"id\":140,\"method\":\"parse\",\"params\":{\"path\":\"composition_basic.c\",\"parse_backend\":\"clang_ast\",\"source\":\"$COMPOSITION_SOURCE\"}}"
    COMPOSITION_RESPONSE_1=$(printf '%s\n' "$COMPOSITION_REQUEST" | "$BIN" --rpc)
    COMPOSITION_RESPONSE_2=$(printf '%s\n' "$COMPOSITION_REQUEST" | "$BIN" --rpc)

    printf '%s\n' "$COMPOSITION_RESPONSE_1" | grep -q '"id":140' || {
        echo "FAIL: composition parse did not echo id 140" >&2
        echo "$COMPOSITION_RESPONSE_1" >&2
        exit 1
    }

    printf '%s\n' "$COMPOSITION_RESPONSE_1" | grep -q '"kind":"composed-contract"' || {
        echo "FAIL: composition_basic.c should yield at least one composed-contract declaration" >&2
        echo "$COMPOSITION_RESPONSE_1" >&2
        exit 1
    }

    # Byte-stable composed CID across runs: composition is deterministic.
    # Extract the composed-contract entry for compose_three (the longest
    # pure chain in the fixture: [double_it, add_one, compose_three]).
    EXTRACT_COMPOSE_THREE_CID='import json,sys; r=json.loads(sys.stdin.read()); ds=r["result"]["declarations"]; cs=[d for d in ds if d.get("kind")=="composed-contract" and d.get("function")=="compose_three"]; print(cs[0]["composedCid"]) if cs else sys.exit(99)'
    CID_1=$(printf '%s\n' "$COMPOSITION_RESPONSE_1" | python3 -c "$EXTRACT_COMPOSE_THREE_CID")
    CID_2=$(printf '%s\n' "$COMPOSITION_RESPONSE_2" | python3 -c "$EXTRACT_COMPOSE_THREE_CID")

    if [ -z "$CID_1" ] || [ -z "$CID_2" ]; then
        echo "FAIL: could not extract compose_three composed CID from lift output" >&2
        echo "$COMPOSITION_RESPONSE_1" >&2
        exit 1
    fi
    if [ "$CID_1" != "$CID_2" ]; then
        echo "FAIL: composed CID for compose_three is not byte-stable across runs (run 1=$CID_1 run 2=$CID_2)" >&2
        exit 1
    fi
    case "$CID_1" in
        blake3-512:*) ;;
        *)
            echo "FAIL: composed CID lacks expected blake3-512 prefix: $CID_1" >&2
            exit 1
            ;;
    esac

    # The pinned compose_three CID guards against accidental changes to
    # the composition pass that would silently break Rust/C federation.
    EXPECTED_COMPOSE_THREE_CID="blake3-512:c636517ab82fac933a920ed47f28f585d7357161c2498c5221928b693cfa7b0dc61570fb62284ac9bb72072992bd84168876be42fefe4e42d03a61a2a94b6e40"
    if [ "$CID_1" != "$EXPECTED_COMPOSE_THREE_CID" ]; then
        echo "FAIL: compose_three composed CID changed; expected $EXPECTED_COMPOSE_THREE_CID got $CID_1" >&2
        exit 1
    fi

    # Position-aware composition per CCP v1.0.0 §9 Rule 1 (singular
    # formal substitution). The formal_position_basic.c fixture has
    # two chains (chain_pos0, chain_pos1) that compose the SAME inner
    # function (`inner`) into the SAME outer function (`outer`) at
    # different formal positions. With the resolver wired in
    # composition.c (pk_c_compose_resolve_formal_idx_in_args), the
    # two chains' composed CIDs MUST differ. A v1 implementation that
    # hardcoded formalIdx=0 would collide them.
    FORMAL_FIXTURE="$SCRIPT_DIR/fixtures/formal_position_basic.c"
    if [ ! -f "$FORMAL_FIXTURE" ]; then
        echo "FAIL: formal-position fixture not found: $FORMAL_FIXTURE" >&2
        exit 1
    fi
    FORMAL_SOURCE=$(sed 's/\\/\\\\/g; s/"/\\"/g; s/$/\\n/' "$FORMAL_FIXTURE" | tr -d '\n' | sed 's/\\n$//')
    FORMAL_REQUEST="{\"jsonrpc\":\"2.0\",\"id\":150,\"method\":\"parse\",\"params\":{\"path\":\"formal_position_basic.c\",\"parse_backend\":\"clang_ast\",\"source\":\"$FORMAL_SOURCE\"}}"
    FORMAL_RESPONSE=$(printf '%s\n' "$FORMAL_REQUEST" | "$BIN" --rpc)

    printf '%s\n' "$FORMAL_RESPONSE" | grep -q '"id":150' || {
        echo "FAIL: formal-position parse did not echo id 150" >&2
        echo "$FORMAL_RESPONSE" >&2
        exit 1
    }

    # Both chains must surface as composed-contract declarations.
    printf '%s\n' "$FORMAL_RESPONSE" | grep -q '"function":"chain_pos0"' || {
        echo "FAIL: formal_position_basic.c should yield a composed-contract for chain_pos0" >&2
        echo "$FORMAL_RESPONSE" >&2
        exit 1
    }
    printf '%s\n' "$FORMAL_RESPONSE" | grep -q '"function":"chain_pos1"' || {
        echo "FAIL: formal_position_basic.c should yield a composed-contract for chain_pos1" >&2
        echo "$FORMAL_RESPONSE" >&2
        exit 1
    }

    EXTRACT_CID_BY_FN='import json,sys; fn=sys.argv[1]; r=json.loads(sys.stdin.read()); ds=r["result"]["declarations"]; cs=[d for d in ds if d.get("kind")=="composed-contract" and d.get("function")==fn]; print(cs[0]["composedCid"]) if cs else sys.exit(99)'
    POS0_CID=$(printf '%s\n' "$FORMAL_RESPONSE" | python3 -c "$EXTRACT_CID_BY_FN" chain_pos0)
    POS1_CID=$(printf '%s\n' "$FORMAL_RESPONSE" | python3 -c "$EXTRACT_CID_BY_FN" chain_pos1)

    if [ -z "$POS0_CID" ] || [ -z "$POS1_CID" ]; then
        echo "FAIL: could not extract composed CIDs for chain_pos0 / chain_pos1" >&2
        echo "$FORMAL_RESPONSE" >&2
        exit 1
    fi

    case "$POS0_CID" in
        blake3-512:*) ;;
        *)
            echo "FAIL: chain_pos0 composed CID lacks blake3-512 prefix: $POS0_CID" >&2
            exit 1
            ;;
    esac
    case "$POS1_CID" in
        blake3-512:*) ;;
        *)
            echo "FAIL: chain_pos1 composed CID lacks blake3-512 prefix: $POS1_CID" >&2
            exit 1
            ;;
    esac

    # Load-bearing assertion: position-aware composition.
    if [ "$POS0_CID" = "$POS1_CID" ]; then
        echo "FAIL: chain_pos0 and chain_pos1 composed CIDs collide ($POS0_CID); position-aware composition is not working. Composition pass is hardcoding formalIdx=0 instead of resolving from each call_site's args layout." >&2
        exit 1
    fi

    # Pin both new CIDs so future changes to the synthetic memento
    # arity / Ctor("tuple") shape are caught (per CCP §6.2 wire
    # format byte-identity requirement).
    EXPECTED_POS0_CID="blake3-512:1e832e9cc5d0cfb4723aec35b81d7af8127a05cd837db035d92e222ee0daab0ec0a42564e701ec25cc01f93d99486212d0a61924ca5868c1615311bb8a8c1ee3"
    EXPECTED_POS1_CID="blake3-512:6f36a5e05cbf645a312b16627ad9c940c43aacc30792c5588a63653ef43a2805b5ad05a89043244944d2fb9e0e758f5e1f0ce1eb7a22ee522270268ee221afd6"
    if [ "$POS0_CID" != "$EXPECTED_POS0_CID" ]; then
        echo "FAIL: chain_pos0 composed CID changed; expected $EXPECTED_POS0_CID got $POS0_CID" >&2
        exit 1
    fi
    if [ "$POS1_CID" != "$EXPECTED_POS1_CID" ]; then
        echo "FAIL: chain_pos1 composed CID changed; expected $EXPECTED_POS1_CID got $POS1_CID" >&2
        exit 1
    fi

    # Tighter signal: the post-formula in each chain must reflect
    # substitution at the correct formal position. chain_pos0
    # substitutes the inner result at outer.formals[0] (= "x0");
    # chain_pos1 substitutes at outer.formals[1] (= "x1"). The
    # synthetic outer atom's post is `result = tuple(x0, x1)`, so
    # after substitution chain_pos0 should contain a Var(x0)
    # replacement and chain_pos1 should contain a Var(x1) replacement.
    # We assert structurally by checking which legacy formal name
    # ("x" from the inner identity post) appears at which position
    # in the bodyJcs-encoded composed post. This is the "PASS =
    # working" CID-divergence assertion the follow-up specs.
    POS0_POST=$(printf '%s\n' "$FORMAL_RESPONSE" | python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
ds = r["result"]["declarations"]
for d in ds:
    if d.get("kind") == "composed-contract" and d.get("function") == "chain_pos0":
        body = json.loads(d["bodyJcs"])
        print(json.dumps(body["post"], sort_keys=True))
        break
')
    POS1_POST=$(printf '%s\n' "$FORMAL_RESPONSE" | python3 -c '
import json, sys
r = json.loads(sys.stdin.read())
ds = r["result"]["declarations"]
for d in ds:
    if d.get("kind") == "composed-contract" and d.get("function") == "chain_pos1":
        body = json.loads(d["bodyJcs"])
        print(json.dumps(body["post"], sort_keys=True))
        break
')
    if [ "$POS0_POST" = "$POS1_POST" ]; then
        echo "FAIL: chain_pos0 / chain_pos1 composed posts identical, formalIdx differential not propagating into substitution" >&2
        echo "post: $POS0_POST" >&2
        exit 1
    fi
fi

echo "provekit-lift-c-kernel-doc integration passed"
