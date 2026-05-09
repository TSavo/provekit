#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'missing required installer dependency: %s\n' "$1" >&2
    exit 2
  fi
}

need_cmd go
need_cmd uv

GOTOOLCHAIN=auto go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@v2.7.1
uv tool install --force in-toto

in_toto_bin_dir="$(uv tool dir --bin)"

if [ -n "${GITHUB_PATH:-}" ]; then
  {
    printf '%s\n' "${HOME}/go/bin"
    printf '%s\n' "$in_toto_bin_dir"
  } >> "$GITHUB_PATH"
fi

printf 'installed slsa-verifier: %s\n' "${HOME}/go/bin/slsa-verifier"
printf 'installed in-toto tools under: %s\n' "$in_toto_bin_dir"
