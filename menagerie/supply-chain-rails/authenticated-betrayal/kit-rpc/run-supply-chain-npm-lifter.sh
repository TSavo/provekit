#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../../.." && pwd)"
TARGET_DIR="${PROVEKIT_SUPPLY_CHAIN_KIT_TARGET_DIR:-$SCRIPT_DIR/target}"
BIN="$TARGET_DIR/debug/supply-chain-npm-lifter"

if [ ! -x "$BIN" ] || [ "$SCRIPT_DIR/supply-chain-npm-lifter.rs" -nt "$BIN" ] || [ "$SCRIPT_DIR/supply-chain-ts-contracts.ts" -nt "$BIN" ] || [ "$SCRIPT_DIR/Cargo.toml" -nt "$BIN" ]; then
  cargo build --quiet --manifest-path "$SCRIPT_DIR/Cargo.toml" --target-dir "$TARGET_DIR" --bin supply-chain-npm-lifter
fi

export PROVEKIT_SUPPLY_CHAIN_KIT_RPC_DIR="$SCRIPT_DIR"
export PROVEKIT_SUPPLY_CHAIN_REPO_ROOT="$REPO_ROOT"
exec "$BIN" "$@"
