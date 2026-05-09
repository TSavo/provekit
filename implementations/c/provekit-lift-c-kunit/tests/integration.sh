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

CONTRACT_COUNT=$(printf '%s\n' "$RESPONSES" | grep -o '"fn_name":"pk_basic_test::[0-9]"' | sort -u | wc -l | tr -d ' ')
if [ "$CONTRACT_COUNT" -ne 4 ]; then
    echo "FAIL: expected 4 KUnit contracts, got $CONTRACT_COUNT" >&2
    echo "$RESPONSES" >&2
    exit 1
fi

for name in pk_basic_test::0 pk_basic_test::1 pk_basic_test::2 pk_basic_test::3; do
    printf '%s\n' "$RESPONSES" | grep -q "\"fn_name\":\"$name\"" || {
        echo "FAIL: missing contract $name" >&2
        echo "$RESPONSES" >&2
        exit 1
    }
done

printf '%s\n' "$RESPONSES" | grep -q '"post":{"kind":"atomic","name":"eq","args":\[{"kind":"var","name":"x"},{"kind":"const","value":5,"sort":{"kind":"primitive","name":"Int"}}\]}' || {
    echo "FAIL: KUNIT_EXPECT_EQ(test, x, 5) did not lift to eq(x, 5)" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"post":{"kind":"atomic","name":"ne","args":\[{"kind":"ctor","name":"add_one","args":\[{"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}}\]},{"kind":"const","value":1,"sort":{"kind":"primitive","name":"Int"}}\]}' || {
    echo "FAIL: KUNIT_EXPECT_NE(test, add_one(1), 1) did not lift to ne(add_one(1), 1)" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"post":{"kind":"atomic","name":"gt","args":\[{"kind":"var","name":"x"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}\]}' || {
    echo "FAIL: KUNIT_EXPECT_TRUE(test, x > 0) did not lift to gt(x, 0)" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf '%s\n' "$RESPONSES" | grep -q '"post":{"kind":"atomic","name":"ne","args":\[{"kind":"var","name":"test"},{"kind":"ctor","name":"NULL","args":\[\]}\]}' || {
    echo "FAIL: KUNIT_EXPECT_NOT_NULL(test, test) did not lift to ne(test, NULL)" >&2
    echo "$RESPONSES" >&2
    exit 1
}

printf 'provekit-lift-c-kunit integration passed: %s contracts from %s KUNIT_* macros\n' "$CONTRACT_COUNT" "$CONTRACT_COUNT"
