#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
specimen_root="$(cd "$script_dir/../.." && pwd)"
repo_root="$(cd "$specimen_root/../../.." && pwd)"

cd "$repo_root"
pnpm exec tsc --noEmit --pretty false -p "$specimen_root/lab/tsconfig.json"
pnpm exec tsx "$specimen_root/lab/harness/src/UserDirectoryHarness.ts"
