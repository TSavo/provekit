#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

target="${1:-cross-language-proof-parity}"
dry_run="$(mktemp)"
trap 'rm -f "$dry_run"' EXIT INT TERM

make -n "$target" >"$dry_run"

require() {
  needle="$1"
  if ! grep -F "$needle" "$dry_run" >/dev/null; then
    echo "FAIL: $target dry-run is missing required Java/Go/Python/Rust parity step: $needle" >&2
    exit 1
  fi
}

reject() {
  pattern="$1"
  label="$2"
  if grep -E "$pattern" "$dry_run" >/dev/null; then
    echo "FAIL: $target dry-run includes $label work; keep extra-language parity in an explicit target" >&2
    grep -En "$pattern" "$dry_run" >&2
    exit 1
  fi
}

require "Cross-language proof parity: emit/materialize/recognize/mint/prove/contradiction lanes for Java, Go, Python, Rust"

require "emit_java_junit_uses_checked_in_java_double_registration"
require "emit_java_testng_uses_checked_in_java_double_registration"
require "emit_go_testing_uses_checked_in_go_double_registration"
require "emit_go_testify_dispatches_separate_emitter_and_compile_checks"
require "emit_python_pytest_uses_checked_in_python_double_registration"
require "emit_rust_cargo_test_uses_checked_in_rust_double_registration"

require "materialize_json_client_jackson_loads_from_proof_and_compiles"
require "go_materialize_uses_checked_in_go_double_realize_registration"
require "go_materialize_uses_body_template_from_go_module_proof"
require "go_dependency_proofs_are_resolved_by_configured_go_kit"
require "materialize_python_uses_checked_in_python_double_realize_registration"
require "materialize_python_requests_example_uses_python_library_shim"
require "test_resolve_dependency_proofs_returns_distribution_proof_bytes"
require "materialize_rust_reqwest_example_uses_rust_library_shim"

require "RecognizeHandlerTest,JavaSugarBindingLifterTest"
require "Test(SugarBody|Recognize)"
require "go_recognize_write_self_resolves_project_proofs_and_proves"
require "sugar_body or recognize"
require "sugar_body -- --nocapture"
require "recognize -- --nocapture"

require "java_mint_auto_writes_body_discharge_bridge"
require "go_mint_auto_writes_body_discharge_bridge"
require "python_mint_auto_writes_body_discharge_bridge"
require "rust_mint_auto_writes_body_discharge_bridge_from_real_lifters"

require "java_production_path_assertion_discharges_and_mints_witness"
require "go_production_path_double_discharges_and_mints_witness"
require "python_production_path_uses_checked_in_python_double_registration"
require "rust_production_path_double_discharges_and_mints_witness"

require "java_production_path_refuses_planted_contradictory_implication"
require "go_production_path_refuses_planted_contradictory_implication"
require "python_production_path_refuses_planted_contradictory_implication"
require "rust_production_path_refuses_planted_contradictory_implication"

reject 'build-ts|cmd_emit_typescript|cmd_verify_typescript|typescript-source/index\.test\.ts' "TypeScript"
reject 'build-zig|provekit-lift-zig-source|cmd_verify_zig|zig build test' "Zig"
reject 'build-scala|cmd_emit_scala|cmd_verify_scala|provekit-lift-scala-source|scala-cli test' "Scala"
reject 'test-swift-source-lift|make test-swift-source-lift' "Swift"

echo "PASS: $target dry-run is scoped to Java/Go/Python/Rust parity"
