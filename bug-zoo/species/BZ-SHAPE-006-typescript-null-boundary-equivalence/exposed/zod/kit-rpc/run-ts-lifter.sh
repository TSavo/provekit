#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
specimen_root="$(cd "$script_dir/../../.." && pwd)"
repo_root="$(cd "$specimen_root/../../.." && pwd)"

cd "$repo_root"
BUG_ZOO_TS_ADAPTER=zod pnpm exec tsx "$specimen_root/tools/ts-boundary-lift-rpc.ts"
