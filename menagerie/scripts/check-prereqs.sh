#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Probe the host toolchain for the runnable Menagerie exhibits.
# Exits 0 when every required tool resolves on PATH; non-zero otherwise.

set -u

REQUIRED=(
  "cargo:Rust runner for cargo run --manifest-path menagerie/<exhibit>/Cargo.toml"
  "node:Required by pnpm and tsx for TypeScript bug-zoo harnesses"
  "pnpm:Drives pnpm exec tsc/tsx in TypeScript bug-zoo harnesses"
  "java:Runs compiled Java bug-zoo harnesses and lifters"
  "javac:Compiles Java bug-zoo lab harnesses"
  "mvn:Builds the Java kit-rpc lifter jars (provekit-lift-java-*)"
  "dotnet:Builds and runs the C# bug-zoo harnesses and lifters"
  "go:Runs the Go side of the BZ-SHAPE-007 polyglot link harness"
  "jq:Used by supply-chain-rails and bridgeworks walkthroughs"
)

OPTIONAL=(
  "cc:Only the bridgeworks walkthrough (C lowerer); not needed for cargo run --all"
  "make:Only the bridgeworks walkthrough; not needed for cargo run --all"
)

probe() {
  local name="$1"
  if command -v "$name" >/dev/null 2>&1; then
    printf '  PASS    %-8s %s\n' "$name" "$(command -v "$name")"
    return 0
  else
    printf '  MISSING %-8s (not on PATH)\n' "$name"
    return 1
  fi
}

missing=()
echo "Required toolchain (cold-start visitors hit these first):"
for entry in "${REQUIRED[@]}"; do
  name="${entry%%:*}"
  reason="${entry#*:}"
  if ! probe "$name"; then
    missing+=("$name ($reason)")
  fi
done

echo
echo "Optional toolchain (walkthrough-only):"
for entry in "${OPTIONAL[@]}"; do
  name="${entry%%:*}"
  probe "$name" >/dev/null
  if command -v "$name" >/dev/null 2>&1; then
    printf '  PASS    %-8s %s\n' "$name" "$(command -v "$name")"
  else
    reason="${entry#*:}"
    printf '  ABSENT  %-8s %s\n' "$name" "$reason"
  fi
done

echo
if [ "${#missing[@]}" -eq 0 ]; then
  echo "All required tools resolve on PATH."
  exit 0
fi

echo "Missing required tools:"
for item in "${missing[@]}"; do
  echo "  - $item"
done
echo
echo "See menagerie/README.md (Prerequisites) for install hints."
exit 1
