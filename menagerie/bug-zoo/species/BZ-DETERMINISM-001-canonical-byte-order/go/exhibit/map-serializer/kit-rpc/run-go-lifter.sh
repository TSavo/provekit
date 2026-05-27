#!/usr/bin/env bash
# Build + exec the REAL go production lifter in verify (core) dialect.
set -euo pipefail
script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../../../../../../../.." && pwd)"
lifter_dir="$repo_root/implementations/go/provekit-lift-go"
bin="$(mktemp -d)/provekit-lift-go"
(cd "$lifter_dir" && go build -o "$bin" ./cmd/provekit-lift-go >&2)
exec "$bin" --dialect=core "$@"
