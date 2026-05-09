#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
EXHIBIT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../../.." && pwd)"
WALKTHROUGH_TARGET_DIR="${PROVEKIT_SWITCHYARD_WALKTHROUGH_TARGET_DIR:-$REPO_ROOT/target/provekit-switchyard-walkthrough-bin}"
WALKTHROUGH_BIN_DIR="$WALKTHROUGH_TARGET_DIR/debug"
PROVEKIT_BIN="$WALKTHROUGH_BIN_DIR/provekit"
SWITCHYARD_BIN="$WALKTHROUGH_BIN_DIR/provekit-protocol-switchyard"

V1_SMUGGLING_SPEC="$EXHIBIT_ROOT/profiles/http/v1/specs/request-smuggling-refusal.md"
V1_FRAMING_SPEC="$EXHIBIT_ROOT/profiles/http/v1/specs/content-length-transfer-encoding.md"
V2_SMUGGLING_SPEC="$EXHIBIT_ROOT/profiles/http/v2/specs/request-smuggling-refusal.md"
V2_FRAMING_SPEC="$EXHIBIT_ROOT/profiles/http/v2/specs/content-length-transfer-encoding.md"

PROTOCOL_NAME="protocol-switchyard-http"
FROM_VERSION="v1.0.0"
TO_VERSION="v1.0.1"
DECLARED_AT="2026-05-09T00:00:00Z"

export CARGO_TERM_COLOR=never
export NO_COLOR=1
export PATH="$WALKTHROUGH_BIN_DIR:$PATH"

section() {
  printf '\n== %s ==\n' "$1"
}

say() {
  printf '%s\n' "$1"
}

pause_for_user() {
  local action="${1:-continue}"
  if [ "${PROVEKIT_SWITCHYARD_WALKTHROUGH_NO_PAUSE:-}" = "1" ] || [ ! -t 0 ]; then
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
  mktemp -d "${TMPDIR:-/tmp}/provekit-switchyard-walkthrough.XXXXXX"
}

show_json_file() {
  local path="$1"
  awk '{ printf "  %6d: %s\n", NR, $0 }' "$path"
}

show_text_file() {
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

print_switchyard() {
  printf '$ provekit-protocol-switchyard'
  printf ' %q' "$@"
  printf '\n'
}

source_newer_than_binary() {
  local binary="$1"
  local source="$2"
  if [ -d "$source" ]; then
    /usr/bin/find "$source" \
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
  if [ "${PROVEKIT_SWITCHYARD_FORCE_REBUILD:-}" = "1" ] || [ ! -x "$binary" ]; then
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
  shift 3
  if binary_needs_build "$binary" "$@"; then
    say "Preparing local binary: $bin_name" >&2
    local log_dir log
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
  if [ "${PROVEKIT_SWITCHYARD_WALKTHROUGH_BINS_READY:-}" = "$WALKTHROUGH_TARGET_DIR" ]; then
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
    "$REPO_ROOT/menagerie/protocol-switchyard/Cargo.toml" \
    "provekit-protocol-switchyard" \
    "$SWITCHYARD_BIN" \
    "$REPO_ROOT/menagerie/protocol-switchyard/src" \
    "$REPO_ROOT/menagerie/protocol-switchyard/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.toml" \
    "$REPO_ROOT/implementations/rust/Cargo.lock"
  export PROVEKIT_SWITCHYARD_WALKTHROUGH_BINS_READY="$WALKTHROUGH_TARGET_DIR"
}

run_provekit_capture() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  ensure_walkthrough_bins
  (cd "$REPO_ROOT" && "$PROVEKIT_BIN" "$@" >"$stdout_file" 2>"$stderr_file")
}

run_switchyard_capture() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  ensure_walkthrough_bins
  (cd "$REPO_ROOT" && "$SWITCHYARD_BIN" "$@" >"$stdout_file" 2>"$stderr_file")
}

hash_spec() {
  local path="$1"
  ensure_walkthrough_bins
  "$PROVEKIT_BIN" hash "$path" 2>/dev/null | tail -1
}

write_v1_catalog() {
  local out="$1"
  local v1_smug_cid v1_fram_cid
  v1_smug_cid="$(hash_spec "$V1_SMUGGLING_SPEC")"
  v1_fram_cid="$(hash_spec "$V1_FRAMING_SPEC")"
  cat > "$out" <<JSON
{
  "kind": "catalog",
  "name": "$PROTOCOL_NAME",
  "version": "$FROM_VERSION",
  "algorithms": {
    "hash": ["blake3-512"],
    "signature": ["ed25519"],
    "pubkey": ["ed25519"]
  },
  "properties": {
    "content-length-transfer-encoding": "$v1_fram_cid",
    "request-smuggling-refusal": "$v1_smug_cid"
  },
  "declaredAt": "$DECLARED_AT"
}
JSON
}

write_v2_catalog() {
  local out="$1"
  local v2_smug_cid v2_fram_cid
  v2_smug_cid="$(hash_spec "$V2_SMUGGLING_SPEC")"
  v2_fram_cid="$(hash_spec "$V2_FRAMING_SPEC")"
  cat > "$out" <<JSON
{
  "kind": "catalog",
  "name": "$PROTOCOL_NAME",
  "version": "$TO_VERSION",
  "algorithms": {
    "hash": ["blake3-512"],
    "signature": ["ed25519"],
    "pubkey": ["ed25519"]
  },
  "properties": {
    "content-length-transfer-encoding": "$v2_fram_cid",
    "request-smuggling-refusal": "$v2_smug_cid"
  },
  "declaredAt": "$DECLARED_AT"
}
JSON
}

write_policy() {
  local out="$1"
  cat > "$out" <<'JSON'
{
  "kind": "ProtocolEvolutionPolicy",
  "schemaVersion": "1",
  "name": "protocol-switchyard-http-v1-to-v2",
  "acceptedChangeClass": "extension-only",
  "versionLabelRule": {
    "extensionOnlyWithoutCrossKitSemanticObligation": "patch"
  },
  "requiredChecks": [
    "from catalog CID recomputes from from-catalog.json",
    "to catalog CID recomputes from to-catalog.json",
    "modified properties are content-addressed by spec file bytes",
    "no core substrate property is changed",
    "change class is extension-only"
  ],
  "nonGoals": [
    "execute grammar conformance against the prose specs",
    "mint an implementation conformance witness for any HTTP server",
    "claim that v2 conformance implies bug-for-bug compatibility with any deployed HTTP stack"
  ]
}
JSON
}

write_verifier() {
  local out="$1"
  cat > "$out" <<'JSON'
{
  "kind": "ProtocolEvolutionVerifier",
  "schemaVersion": "1",
  "name": "protocol-switchyard-http-verifier",
  "version": "0.1.0",
  "acceptedTools": [
    {
      "name": "blake3-512 of spec bytes",
      "command": "cargo run --manifest-path menagerie/protocol-switchyard/Cargo.toml -- --all",
      "purpose": "Recompute property CIDs by hashing the spec files in this exhibit."
    }
  ],
  "acceptedManualAssertions": [
    "v2 strengthens the v1 framing-ambiguity refusal by closing additional boundary cases.",
    "v1 conformance witnesses do not imply v2 conformance.",
    "this exhibit demonstrates the witnessed-edge shape, not implementation conformance."
  ]
}
JSON
}

mint_evolution_into() {
  local work_dir="$1"
  local from_path="$work_dir/from-catalog.json"
  local to_path="$work_dir/to-catalog.json"
  local policy_path="$work_dir/policy.json"
  local verifier_path="$work_dir/verifier.json"
  local witness_dir="$work_dir/witness"
  local stdout_file="$work_dir/evolve.stdout"
  local stderr_file="$work_dir/evolve.stderr"

  write_v1_catalog "$from_path"
  write_v2_catalog "$to_path"
  write_policy "$policy_path"
  write_verifier "$verifier_path"
  mkdir -p "$witness_dir"

  run_provekit_capture "$stdout_file" "$stderr_file" \
    protocol evolve \
    --from "$from_path" \
    --to "$to_path" \
    --policy "$policy_path" \
    --verifier "$verifier_path" \
    --out-dir "$witness_dir" \
    --change-class extension-only \
    --producer protocol-switchyard-walkthrough \
    --changed-spec "request-smuggling-refusal=$V2_SMUGGLING_SPEC" \
    --changed-spec "content-length-transfer-encoding=$V2_FRAMING_SPEC" \
    --json --quiet
}

next_script() {
  local next="$1"
  section "Next"
  say "Run: $SCRIPT_DIR/$next"
}
