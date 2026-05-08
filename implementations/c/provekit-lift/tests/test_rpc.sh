#!/bin/sh
set -eu

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/checked_add_u8.c" <<'C_EOF'
#include <stdbool.h>
#include <stdint.h>

typedef struct {
    bool overflow;
    uint8_t value;
} checked_add_u8_result;

/* provekit:contract checked_add_u8.postcondition */
checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {
    uint16_t wide = (uint16_t)a + (uint16_t)b;
    if (wide >= 256) {
        return (checked_add_u8_result){ .overflow = true, .value = 0 };
    }
    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };
}
C_EOF

responses="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$tmp"
        printf ',"source_paths":["."],"surface":"c"}}\n'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"shutdown"}'
    } | ./bin/provekit-lift-c --rpc
)"

printf '%s\n' "$responses" | grep '"name":"c-lift"' >/dev/null
printf '%s\n' "$responses" | grep '"kind":"ir-document"' >/dev/null
printf '%s\n' "$responses" | grep '"name":"checked_add_u8.postcondition"' >/dev/null

bad="$(mktemp -d)"
cat > "$bad/checked_add_u8.c" <<'C_EOF'
#include <stdbool.h>
#include <stdint.h>

typedef struct {
    bool overflow;
    uint8_t value;
} checked_add_u8_result;

/* provekit:contract checked_add_u8.postcondition */
checked_add_u8_result checked_add_u8(uint8_t a, uint8_t b) {
    uint16_t wide = (uint16_t)a + (uint16_t)b;
    return (checked_add_u8_result){ .overflow = false, .value = (uint8_t)wide };
}
C_EOF

bad_responses="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":'
        printf '"%s"' "$bad"
        printf ',"source_paths":["."],"surface":"c"}}\n'
    } | ./bin/provekit-lift-c --rpc
)"
rm -rf "$bad"

printf '%s\n' "$bad_responses" | grep '"error"' >/dev/null
printf '%s\n' "$bad_responses" | grep 'checked_add_u8.postcondition' >/dev/null
