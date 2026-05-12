#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/../provekit-lift-c-kunit"
FIXTURE="$SCRIPT_DIR/fixtures/kunit_basic.c"

if [ ! -x "$BIN" ]; then
    echo "FAIL: binary not found: $BIN" >&2
    exit 1
fi

if [ ! -f "$FIXTURE" ]; then
    echo "FAIL: fixture not found: $FIXTURE" >&2
    exit 1
fi

RESPONSES="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$SCRIPT_DIR/fixtures"
        printf ',"source_paths":["kunit_basic.c"],"surface":"c-kunit"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | "$BIN" --rpc
)"

printf '%s\n' "$RESPONSES" | grep -q '"name":"c-kunit"' || {
    echo "FAIL: initialize missing c-kunit name" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"protocol_version":"pep/1.7.0"' || {
    echo "FAIL: initialize missing protocol version" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"kind":"ir-document"' || {
    echo "FAIL: lift missing ir-document kind" >&2
    echo "$RESPONSES" >&2
    exit 1
}

CONTRACT_COUNT=$(printf '%s\n' "$RESPONSES" | grep -o '"fn_name":"pk_basic_test::[0-9]"' | sort -u | wc -l | tr -d ' ')
if [ "$CONTRACT_COUNT" -ne 0 ]; then
    echo "FAIL: KUnit contracts must not attach to pk_basic_test::<index>" >&2
    echo "$RESPONSES" >&2
    exit 1
fi

DIRECT_LINE=$(grep -n 'KUNIT_EXPECT_EQ(test, foo(5), 10)' "$FIXTURE" | cut -d: -f1)
BOUND_LINE=$(grep -n 'int r = foo(5);' "$FIXTURE" | cut -d: -f1)
DIRECT_NAME="foo@kunit_basic.c:$DIRECT_LINE"
BOUND_NAME="foo@kunit_basic.c:$BOUND_LINE"

for name in "$DIRECT_NAME" "$BOUND_NAME"; do
    printf '%s\n' "$RESPONSES" | grep -q "\"name\":\"$name\"" || {
        echo "FAIL: missing callsite-named contract $name" >&2
        echo "$RESPONSES" >&2
        exit 1
    }
done

CONTRACT_COUNT=$(printf '%s\n' "$RESPONSES" | grep -o '"name":"foo@kunit_basic.c:[0-9][0-9]*"' | sort -u | wc -l | tr -d ' ')
if [ "$CONTRACT_COUNT" -ne 2 ]; then
    echo "FAIL: expected 2 callsite-named foo contracts, got $CONTRACT_COUNT" >&2
    echo "$RESPONSES" >&2
    exit 1
fi

printf '%s\n' "$RESPONSES" | grep -q '"fn_name":"foo"' || {
    echo "FAIL: expected contracts attached to fn_name foo" >&2
    echo "$RESPONSES" >&2
    exit 1
}

FORMULA_COUNT=$(printf '%s\n' "$RESPONSES" | grep -o '"post":{"kind":"atomic","name":"eq","args":\[{"kind":"ctor","name":"foo","args":\[{"kind":"const","value":5,"sort":{"kind":"primitive","name":"Int"}}\]},{"kind":"const","value":10,"sort":{"kind":"primitive","name":"Int"}}\]}' | wc -l | tr -d ' ')
if [ "$FORMULA_COUNT" -lt 2 ]; then
    echo "FAIL: expected direct and let-bound foo(5) == 10 formulas, got $FORMULA_COUNT" >&2
    echo "$RESPONSES" >&2
    exit 1
fi

printf '%s\n' "$RESPONSES" | grep -q '"kind":"c-kunit.unsupported-assertion"' &&
    printf '%s\n' "$RESPONSES" | grep -q 'KUnit assertion did not identify a callsite' || {
    echo "FAIL: literal-bound KUNIT_EXPECT_EQ(test, x, 5) did not log a skip warning" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf 'provekit-lift-c-kunit integration passed: %s callsite contracts\n' "$CONTRACT_COUNT"
