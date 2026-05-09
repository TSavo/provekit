#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="${PROVEKIT_SUPPLY_CHAIN_KIT_TARGET_DIR:-$SCRIPT_DIR/target}"
BIN="$TARGET_DIR/debug/supply-chain-js-lowerer"

if [ ! -x "$BIN" ] || [ "$SCRIPT_DIR/supply-chain-js-lowerer.rs" -nt "$BIN" ] || [ "$SCRIPT_DIR/Cargo.toml" -nt "$BIN" ]; then
  cargo build --quiet --manifest-path "$SCRIPT_DIR/Cargo.toml" --target-dir "$TARGET_DIR" --bin supply-chain-js-lowerer
fi

exec "$BIN" "$@"
