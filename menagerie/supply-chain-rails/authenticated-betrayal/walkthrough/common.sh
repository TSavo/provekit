#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
EXHIBIT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../../.." && pwd)"
WALKTHROUGH_TARGET_DIR="${PROVEKIT_SUPPLY_CHAIN_WALKTHROUGH_TARGET_DIR:-$REPO_ROOT/target/provekit-supply-chain-walkthrough-bin}"
WALKTHROUGH_BIN_DIR="$WALKTHROUGH_TARGET_DIR/debug"
PROVEKIT_BIN="$WALKTHROUGH_BIN_DIR/provekit"
RUNNER_BIN="$WALKTHROUGH_BIN_DIR/provekit-supply-chain-rails"

export CARGO_TERM_COLOR=never
export NO_COLOR=1
export PATH="$WALKTHROUGH_BIN_DIR:$HOME/go/bin:$HOME/.local/bin:$PATH"
export PROVEKIT_CLI="$PROVEKIT_BIN"
export PROVEKIT_SUPPLY_CHAIN_EXTERNAL_CLI=1
export PROVEKIT_SUPPLY_CHAIN_KIT_TARGET_DIR="$WALKTHROUGH_TARGET_DIR/kit-rpc"

section() {
  printf '\n== %s ==\n' "$1"
}

say() {
  printf '%s\n' "$1"
}

pause_for_user() {
  local action="${1:-continue}"
  if [ "${PROVEKIT_SUPPLY_CHAIN_WALKTHROUGH_NO_PAUSE:-}" = "1" ] || [ ! -t 0 ]; then
    return
  fi
  printf '\nPress Enter to %s...' "$action"
  IFS= read -r _
  printf '\n'
}

explain_then_pause() {
  local action="${1:-continue}"
  section "What This Means"
  cat
  pause_for_user "$action"
}

analysis_with_receipts() {
  section "Human Analysis With Receipts"
  cat
}

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'missing required command: %s\n' "$1" >&2
    exit 2
  fi
}

need_jq() {
  need_cmd jq
}

tmp_dir() {
  mktemp -d "${TMPDIR:-/tmp}/provekit-supply-chain-walkthrough.XXXXXX"
}

show_json_file() {
  local path="$1"
  awk '{ printf "  %6d: %s\n", NR, $0 }' "$path"
}

highlight_raw_line() {
  local path="$1"
  local pattern="$2"
  local comment="$3"
  local found=0
  local line text

  while IFS=: read -r line text; do
    found=1
    printf '  %6d: %s\n' "$line" "$text"
  done < <(grep -n -F "$pattern" "$path" || true)
  if [ "$found" -eq 0 ]; then
    say "  missing raw line matching: $pattern"
    return 1
  fi
  say "  comment: $comment"
}

print_provekit() {
  printf '$ provekit'
  printf ' %q' "$@"
  printf '\n'
}

print_runner() {
  printf '$ provekit-supply-chain-rails'
  printf ' %q' "$@"
  printf '\n'
}

source_newer_than_binary() {
  local binary="$1"
  local source="$2"
  if [ -d "$source" ]; then
    find "$source" \
      \( -path '*/target' -o -path '*/.git' \) -prune -o \
      -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
      -newer "$binary" -print -quit | grep -q .
    return
  fi
  [ -f "$source" ] && [ "$source" -nt "$binary" ]
}

binary_needs_build() {
  local binary="$1"
  shift
  if [ "${PROVEKIT_SUPPLY_CHAIN_FORCE_REBUILD:-}" = "1" ] || [ ! -x "$binary" ]; then
    return 0
  fi
  local source
  for source in "$@"; do
    if source_newer_than_binary "$binary" "$source"; then
      return 0
    fi
  done
  return 1
}

build_cargo_binary_if_needed() {
  local manifest="$1"
  local bin_name="$2"
  local binary="$3"
  local log_dir log
  shift 3
  if binary_needs_build "$binary" "$@"; then
    log_dir="$(tmp_dir)"
    log="$log_dir/build-$bin_name.log"
    if ! cargo build --quiet --manifest-path "$manifest" --target-dir "$WALKTHROUGH_TARGET_DIR" --bin "$bin_name" >"$log" 2>&1; then
      cat "$log" >&2
      return 1
    fi
    rm -rf "$log_dir"
  fi
}

ensure_walkthrough_bins() {
  if [ "${PROVEKIT_SUPPLY_CHAIN_WALKTHROUGH_BINS_READY:-}" = "$WALKTHROUGH_TARGET_DIR" ]; then
    return
  fi
  need_cmd cargo
  mkdir -p "$WALKTHROUGH_TARGET_DIR"
  build_cargo_binary_if_needed \
    "$REPO_ROOT/implementations/rust/provekit-cli/Cargo.toml" \
    "provekit" \
    "$PROVEKIT_BIN" \
    "$REPO_ROOT/implementations/rust/provekit-cli/src" \
    "$REPO_ROOT/implementations/rust/provekit-cli/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.lock"
  build_cargo_binary_if_needed \
    "$REPO_ROOT/menagerie/supply-chain-rails/Cargo.toml" \
    "provekit-supply-chain-rails" \
    "$RUNNER_BIN" \
    "$REPO_ROOT/menagerie/supply-chain-rails/src" \
    "$REPO_ROOT/menagerie/supply-chain-rails/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.lock"
  export PROVEKIT_SUPPLY_CHAIN_WALKTHROUGH_BINS_READY="$WALKTHROUGH_TARGET_DIR"
}

run_provekit_capture() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  ensure_walkthrough_bins
  (cd "$REPO_ROOT" && "$PROVEKIT_BIN" "$@" >"$stdout_file" 2>"$stderr_file")
}

run_runner_capture() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  ensure_walkthrough_bins
  (cd "$REPO_ROOT" && "$RUNNER_BIN" "$@" >"$stdout_file" 2>"$stderr_file")
}

next_script() {
  local script="$1"
  section "Next"
  say "./$script"
}
