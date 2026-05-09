#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"

tmp="$(tmp_dir)"
inspect_json="$tmp/conventional-green.json"
inspect_stderr="$tmp/conventional-green.stderr"

section "Show Conventional Green"
explain_then_pause "inspect safe-json 1.4.2 as a package manager would" <<'TEXT'
Now the exhibit changes the package behavior. safe-json@1.4.2 is still the right package, right version shape, right tarball metadata, and right maintainer story. The conventional receipts are deliberately green. This is not a strawman failure where metadata is obviously broken.

ProvekIt value starts with refusing to overclaim. The package inspection receipt is not stamping labels. It invokes `slsa-verifier verify-vsa` against a signed SLSA Verification Summary Attestation for the tarball digest, and it invokes `in-toto-verify` against a signed root layout plus the packaging link. Those tools go green, and ProvekIt records their commands and receipt paths.

That still is not admission. SLSA says a verifier accepted the subject digest at a declared level. in-toto says the packaging step produced `package.tgz` from the declared materials under the authorized functionary key. Neither receipt says the JavaScript runtime preserves `runtime.no-env-secret-read`.

What to look for: the package is a real npm tarball, SLSA and in-toto name their actual tools, both receipts are green, and admission.status remains not-decided.
TEXT

print_provekit package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --json --quiet
run_provekit_capture "$inspect_json" "$inspect_stderr" package inspect "menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.2-lie" --json --quiet

section "Conventional Green JSON"
show_json_file "$inspect_json"
highlight_raw_line "$inspect_json" '"format": "npm-pack-tarball"' "The artifact is a real npm pack tarball, not a text placeholder."
highlight_raw_line "$inspect_json" '"tool": "slsa-verifier"' "The SLSA rail came from the installed SLSA verifier."
highlight_raw_line "$inspect_json" '"predicateType": "https://slsa.dev/verification_summary/v1"' "The SLSA receipt is a signed Verification Summary Attestation."
highlight_raw_line "$inspect_json" '"tool": "in-toto-verify"' "The in-toto rail came from the installed in-toto verifier."
highlight_raw_line "$inspect_json" '"productDigestMatchesArtifact": true' "ProvekIt checked that the in-toto product digest names the observed tarball."
highlight_raw_line "$inspect_json" '"sbomContents": {' "The SBOM-style contents receipt is recorded as a receipt object."
highlight_raw_line "$inspect_json" '"status": "not-decided"' "ProvekIt does not confuse provenance with contract admission."

analysis_with_receipts <<'TEXT'
These lines prove the setup. The release can be authentic and conventional-green while still not being admitted. SLSA and in-toto are not mocked here: ProvekIt invoked their installed verifiers and preserved the exact receipt shape in the package inspection output.

That gap is exactly where ProvekIt adds value: the maintainer can claim the old contract, but the claim has to lower into evidence.
TEXT

next_script "04-preserve-contracts-fail-witness.sh"
