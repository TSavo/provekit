#!/bin/sh
set -eu

target_dir="${PROVEKIT_BRIDGEWORKS_KIT_TARGET_DIR:-}"
if [ -z "$target_dir" ]; then
  target_dir="${TMPDIR:-/tmp}/provekit-bridgeworks-kit-rpc-target"
fi

bin_dir="${PROVEKIT_BRIDGEWORKS_KIT_BIN_DIR:-}"
if [ -n "$bin_dir" ] && [ -x "$bin_dir/bridgeworks-checked-add-lifter" ]; then
  exec "$bin_dir/bridgeworks-checked-add-lifter" "$@"
fi

if command -v bridgeworks-checked-add-lifter >/dev/null 2>&1; then
  exec bridgeworks-checked-add-lifter "$@"
fi

cargo build --quiet --manifest-path kit-rpc/Cargo.toml --target-dir "$target_dir" --bin bridgeworks-checked-add-lifter
exec "$target_dir/debug/bridgeworks-checked-add-lifter" "$@"
