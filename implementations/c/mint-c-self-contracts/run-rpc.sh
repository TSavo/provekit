#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
#
# Launcher for the c-self-contracts lift surface. The lift manifest spawns
# this script (relative to implementations/c); we resolve the binary
# relative to this script's directory so the manifest's working_dir
# doesn't matter for binary discovery.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/mint-c-self-contracts"

if [ ! -x "$BIN" ]; then
    echo "ERROR: binary not built at $BIN. Run \`make build-c-self-contracts\`." >&2
    exit 1
fi

exec "$BIN" --rpc "$@"
