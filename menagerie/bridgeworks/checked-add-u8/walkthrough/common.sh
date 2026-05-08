#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
EXHIBIT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../../.." && pwd)"
WALKTHROUGH_TARGET_DIR="${PROVEKIT_BRIDGEWORKS_WALKTHROUGH_TARGET_DIR:-$REPO_ROOT/target/provekit-bridgeworks-walkthrough-bin}"
WALKTHROUGH_BIN_DIR="$WALKTHROUGH_TARGET_DIR/debug"
PROVEKIT_BIN="$WALKTHROUGH_BIN_DIR/provekit"
BRIDGEWORKS_BIN="$WALKTHROUGH_BIN_DIR/provekit-bridgeworks"
BRIDGEWORKS_LIFTER_BIN="$WALKTHROUGH_BIN_DIR/bridgeworks-checked-add-lifter"
BRIDGEWORKS_C_LOWERER_BIN="$WALKTHROUGH_BIN_DIR/bridgeworks-c-witness-lowerer"

export CARGO_TERM_COLOR=never
export NO_COLOR=1
export PATH="$WALKTHROUGH_BIN_DIR:$PATH"
export PROVEKIT_CLI="$PROVEKIT_BIN"
export PROVEKIT_BRIDGEWORKS_EXTERNAL_CLI=1
export PROVEKIT_BRIDGEWORKS_KIT_BIN_DIR="$WALKTHROUGH_BIN_DIR"
export PROVEKIT_BRIDGEWORKS_KIT_TARGET_DIR="$WALKTHROUGH_TARGET_DIR"

section() {
  printf '\n== %s ==\n' "$1"
}

say() {
  printf '%s\n' "$1"
}

pause_for_user() {
  local action="${1:-continue}"

  if [ "${PROVEKIT_BRIDGEWORKS_WALKTHROUGH_NO_PAUSE:-}" = "1" ] || [ ! -t 0 ]; then
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

strip_ansi() {
  sed $'s/\x1b\\[[0-9;]*m//g'
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
  mktemp -d "${TMPDIR:-/tmp}/provekit-bridgeworks-walkthrough.XXXXXX"
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

show_mutation_diff() {
  local original="$1"
  local mutated="$2"
  local rel="$3"
  local code

  section "Mutation Diff"
  say "Raw unified diff: original exhibit artifact -> mutated temp copy."
  set +e
  diff -u --label "original/$rel" --label "mutated/$rel" "$original" "$mutated"
  code=$?
  set -e
  if [ "$code" -eq 0 ]; then
    say "  no diff"
  elif [ "$code" -ne 1 ]; then
    return "$code"
  fi
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

  if [ "${PROVEKIT_BRIDGEWORKS_FORCE_REBUILD:-}" = "1" ]; then
    return 0
  fi

  if [ ! -x "$binary" ]; then
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
    say "Preparing local binary: $bin_name" >&2
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
  if [ "${PROVEKIT_BRIDGEWORKS_WALKTHROUGH_BINS_READY:-}" = "$WALKTHROUGH_TARGET_DIR" ]; then
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
    "$REPO_ROOT/implementations/rust/provekit-agent/src" \
    "$REPO_ROOT/implementations/rust/provekit-agent/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-canonicalizer/src" \
    "$REPO_ROOT/implementations/rust/provekit-canonicalizer/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-claim-envelope/src" \
    "$REPO_ROOT/implementations/rust/provekit-claim-envelope/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-ir-symbolic/src" \
    "$REPO_ROOT/implementations/rust/provekit-ir-symbolic/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-lift/src" \
    "$REPO_ROOT/implementations/rust/provekit-lift/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-linker/src" \
    "$REPO_ROOT/implementations/rust/provekit-linker/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-policy/src" \
    "$REPO_ROOT/implementations/rust/provekit-policy/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-proof-envelope/src" \
    "$REPO_ROOT/implementations/rust/provekit-proof-envelope/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-self-contracts/src" \
    "$REPO_ROOT/implementations/rust/provekit-self-contracts/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/provekit-verifier/src" \
    "$REPO_ROOT/implementations/rust/provekit-verifier/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/libprovekit/src" \
    "$REPO_ROOT/implementations/rust/libprovekit/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.lock"
  build_cargo_binary_if_needed \
    "$REPO_ROOT/menagerie/bridgeworks/Cargo.toml" \
    "provekit-bridgeworks" \
    "$BRIDGEWORKS_BIN" \
    "$REPO_ROOT/menagerie/bridgeworks/src" \
    "$REPO_ROOT/menagerie/bridgeworks/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.lock"
  if binary_needs_build "$BRIDGEWORKS_LIFTER_BIN" \
    "$EXHIBIT_ROOT/kit-rpc/bridgeworks-lifter.rs" \
    "$EXHIBIT_ROOT/kit-rpc/Cargo.toml" \
    "$EXHIBIT_ROOT/kit-rpc/Cargo.lock" \
    || binary_needs_build "$BRIDGEWORKS_C_LOWERER_BIN" \
      "$EXHIBIT_ROOT/kit-rpc/bridgeworks-c-witness-lowerer.rs" \
      "$EXHIBIT_ROOT/kit-rpc/Cargo.toml" \
      "$EXHIBIT_ROOT/kit-rpc/Cargo.lock"; then
    local log_dir log
    say "Preparing local binaries: bridgeworks-checked-add-lifter, bridgeworks-c-witness-lowerer" >&2
    log_dir="$(tmp_dir)"
    log="$log_dir/build-kit-rpc.log"
    if ! cargo build --quiet --manifest-path "$EXHIBIT_ROOT/kit-rpc/Cargo.toml" --target-dir "$WALKTHROUGH_TARGET_DIR" --bins >"$log" 2>&1; then
      cat "$log" >&2
      return 1
    fi
    rm -rf "$log_dir"
  fi

  export PROVEKIT_BRIDGEWORKS_WALKTHROUGH_BINS_READY="$WALKTHROUGH_TARGET_DIR"
}

print_provekit() {
  printf '$ provekit'
  printf ' %q' "$@"
  printf '\n'
}

run_provekit_capture() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  ensure_walkthrough_bins
  (cd "$REPO_ROOT" && "$PROVEKIT_BIN" "$@" >"$stdout_file" 2>"$stderr_file")
}

print_bridgeworks() {
  printf '$ provekit-bridgeworks'
  printf ' %q' "$@"
  printf '\n'
}

run_bridgeworks_capture() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  ensure_walkthrough_bins
  (cd "$REPO_ROOT" && "$BRIDGEWORKS_BIN" "$@" >"$stdout_file" 2>"$stderr_file")
}

copy_exhibit_to_temp() {
  local root
  mkdir -p "$REPO_ROOT/target"
  root="$(mktemp -d "$REPO_ROOT/target/provekit-bridgeworks-walkthrough.XXXXXX")"
  mkdir -p "$root/exhibit"
  (
    cd "$EXHIBIT_ROOT"
    tar --exclude './kit-rpc/target' --exclude './target' --exclude './out' -cf - .
  ) | (
    cd "$root/exhibit"
    tar -xf -
  )
  printf '%s\n' "$root/exhibit"
}

write_witness_plan() {
  local path="$1"
  cat >"$path" <<'JSON'
{
  "surface": "c",
  "mode": "witness",
  "attachTo": "checked_add_u8.postcondition",
  "obligation": {
    "kind": "predicate",
    "name": "checked_add_u8.postcondition"
  },
  "host": {
    "kit": "c",
    "contextKind": "source-artifact",
    "artifact": "artifacts/software/checked_add_u8.c",
    "entrypoint": "checked_add_u8"
  },
  "bindings": [
    {"proofVar": "a", "hostPath": "parameter:a", "typeHint": "uint8_t"},
    {"proofVar": "b", "hostPath": "parameter:b", "typeHint": "uint8_t"},
    {"proofVar": "out", "hostPath": "return", "typeHint": "checked_add_u8_result"}
  ],
  "policy": {
    "policyCid": "builtin:bridgeworks.checked-add-u8.exhaustive-u8",
    "mode": "exhaustive-u8"
  }
}
JSON
}

mint_positive() {
  local dir="$1"
  mkdir -p "$dir"
  print_provekit mint --project "menagerie/bridgeworks/checked-add-u8" --out "$dir" --no-attest --json --quiet
  run_provekit_capture "$dir/mint.json" "$dir/mint.stderr" mint --project "menagerie/bridgeworks/checked-add-u8" --out "$dir" --no-attest --json --quiet
}

run_mutation_expect_refusal() {
  local id="$1"
  local source_rel="$2"
  local target_rel="$3"
  local expected="$4"
  local tmp project out_dir stdout_file stderr_file status

  tmp="$(tmp_dir)"
  project="$(copy_exhibit_to_temp)"
  out_dir="$tmp/out"
  stdout_file="$tmp/mint.stdout"
  stderr_file="$tmp/mint.stderr"

  mkdir -p "$(dirname "$project/$target_rel")" "$out_dir"
  cp "$EXHIBIT_ROOT/$source_rel" "$project/$target_rel"

  say "Mutation: $id"
  say "Mutated copy: $project"
  say "Expected refusal contains: $expected"
  show_mutation_diff "$EXHIBIT_ROOT/$target_rel" "$project/$target_rel" "$target_rel"
  print_provekit mint --project "$project" --out "$out_dir" --no-attest --json --quiet

  set +e
  run_provekit_capture "$stdout_file" "$stderr_file" mint --project "$project" --out "$out_dir" --no-attest --json --quiet
  status=$?
  set -e

  if [ "$status" -eq 0 ]; then
    say "Unexpectedly accepted mutation. Mint output:"
    cat "$stdout_file"
    exit 1
  fi

  section "Refusal Evidence"
  if grep -F "$expected" "$stdout_file" "$stderr_file" >/dev/null; then
    say "Matched expected refusal: $expected"
  else
    say "Did not find expected refusal. Raw stdout/stderr paths:"
    say "  $stdout_file"
    say "  $stderr_file"
    exit 1
  fi

  grep -F "error:" "$stderr_file" | tail -1 | strip_ansi || true
  grep -F "$expected" "$stdout_file" "$stderr_file" | head -5 | strip_ansi || true
  grep -E "counterexample:|expected:|observed:|needed:|software emitted:" "$stderr_file" | head -20 | strip_ansi || true
}

next_script() {
  local next="$1"
  section "Next"
  say "Run: $SCRIPT_DIR/$next"
}
