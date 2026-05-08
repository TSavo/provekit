#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
lab_root="$(cd "$script_dir/.." && pwd)"
classes="$script_dir/.classes"

rm -rf "$classes"
mkdir -p "$classes"

javac -d "$classes" \
  "$lab_root/library/src/main/java/zoo/AmountParser.java" \
  "$script_dir/src/main/java/zoo/AmountFlowHarness.java"

java -cp "$classes" zoo.AmountFlowHarness
