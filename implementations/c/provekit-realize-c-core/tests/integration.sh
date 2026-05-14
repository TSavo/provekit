#!/bin/sh
set -eu

bin="./target/release/provekit-realize-c"

if [ ! -x "$bin" ]; then
    printf 'missing executable: %s\n' "$bin" >&2
    exit 1
fi

responses="$(
    {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"provekit.plugin.invoke","params":{"function":"wrap_identity","params":["x"],"param_types":["int"],"return_type":"int","concept_name":"identity"}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"provekit.plugin.invoke","params":{"function":"free_p","params":["p"],"param_types":["int *"],"return_type":"void","concept_name":"free"}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":4,"method":"provekit.plugin.invoke","params":{"function":"bad_sort","params":["x"],"param_types":["int"],"return_type":"MysterySort","concept_name":"identity"}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":5,"method":"shutdown","params":{}}'
    } | "$bin" --rpc
)"

printf '%s\n' "$responses" | grep '"name":"provekit-realize-c"' >/dev/null
printf '%s\n' "$responses" | grep '"protocol_version":"pep/1.7.0"' >/dev/null
printf '%s\n' "$responses" | grep '"authoring_surfaces":\["c","c11"\]' >/dev/null
printf '%s\n' "$responses" | grep '"id":2' >/dev/null
printf '%s\n' "$responses" | grep '"source":"int wrap_identity(int x) {\\n    return x;\\n}\\n"' >/dev/null
printf '%s\n' "$responses" | grep '"is_stub":false' >/dev/null
printf '%s\n' "$responses" | grep '"id":3' >/dev/null
printf '%s\n' "$responses" | grep '"source":"void free_p(int \*p) {\\n    free(p);\\n}\\n"' >/dev/null
printf '%s\n' "$responses" | grep '"id":4' >/dev/null
printf '%s\n' "$responses" | grep '"error":{"code":-32602,"message":"UNSUPPORTED_SORT: no C type mapping for MysterySort"}' >/dev/null
