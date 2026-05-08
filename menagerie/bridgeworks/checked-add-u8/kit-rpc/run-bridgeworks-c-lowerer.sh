#!/bin/sh
set -eu

target_dir="${PROVEKIT_BRIDGEWORKS_KIT_TARGET_DIR:-}"
if [ -z "$target_dir" ]; then
  target_dir="${TMPDIR:-/tmp}/provekit-bridgeworks-kit-rpc-target"
fi

bin_dir="${PROVEKIT_BRIDGEWORKS_KIT_BIN_DIR:-}"
if [ -n "$bin_dir" ] && [ -x "$bin_dir/bridgeworks-c-witness-lowerer" ]; then
  exec "$bin_dir/bridgeworks-c-witness-lowerer" "$@"
fi

if command -v bridgeworks-c-witness-lowerer >/dev/null 2>&1; then
  exec bridgeworks-c-witness-lowerer "$@"
fi

cargo build --quiet --manifest-path kit-rpc/Cargo.toml --target-dir "$target_dir" --bin bridgeworks-c-witness-lowerer
exec "$target_dir/debug/bridgeworks-c-witness-lowerer" "$@"
