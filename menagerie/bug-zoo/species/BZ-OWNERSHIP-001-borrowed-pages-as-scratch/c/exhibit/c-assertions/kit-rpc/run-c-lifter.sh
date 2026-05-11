#!/usr/bin/env bash
# BZ-OWNERSHIP-001 c-assertions exhibit kit-rpc: invoke the C assertions lifter.
#
# Layout:  menagerie/bug-zoo/species/<species>/c/exhibit/c-assertions/kit-rpc/
# Repo root is 8 levels up from this script.
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../../../../../../.." && pwd)"

lifter_dir="$repo_root/implementations/c/provekit-lift-c-assertions"
lifter_bin="$lifter_dir/provekit-lift-c-assertions"

if [[ ! -x "$lifter_bin" ]]; then
    (cd "$lifter_dir" && make >&2)
fi

if [[ ! -x "$lifter_bin" ]]; then
    echo "c-assertions lifter binary not found at: $lifter_bin" >&2
    echo "Build with: cd implementations/c/provekit-lift-c-assertions && make" >&2
    exit 1
fi

exec "$lifter_bin" "$@"
