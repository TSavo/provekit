#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0

source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/common.sh"
need_jq

tmp="$(tmp_dir)"
inspect_json="$tmp/baseline-inspect.json"
inspect_stderr="$tmp/baseline-inspect.stderr"
mint_json="$tmp/baseline-mint.json"
mint_stderr="$tmp/baseline-mint.stderr"
post_proof_inspect_json="$tmp/baseline-post-proof-inspect.json"
post_proof_inspect_stderr="$tmp/baseline-post-proof-inspect.stderr"
baseline_rel="menagerie/supply-chain-rails/authenticated-betrayal/packages/safe-json-1.4.1"
baseline_pkg="$REPO_ROOT/$baseline_rel"
work_pkg="$tmp/safe-json-1.4.1-with-proof"
cp -R "$baseline_pkg" "$work_pkg"
perl -0pi -e "s#../../kit-rpc/run-supply-chain-npm-lifter.sh#$EXHIBIT_ROOT/kit-rpc/run-supply-chain-npm-lifter.sh#g" \
  "$work_pkg/.provekit/lift/supply-chain-npm/manifest.toml"
perl -0pi -e "s#../../kit-rpc/run-supply-chain-js-lowerer.sh#$EXHIBIT_ROOT/kit-rpc/run-supply-chain-js-lowerer.sh#g" \
  "$work_pkg/.provekit/lower/javascript/manifest.toml" \
  "$work_pkg/.provekit/lower/package-manifest/manifest.toml"

section "Admit Baseline"
explain_then_pause "inspect and mint safe-json 1.4.1" <<'TEXT'
ProvekIt first inspects the boring baseline release. This is package-shaped work: read package.json, read the package tarball fixture, compute the binary CID, compute the CI input-closure CID, and surface the contract set carried by the package.

Then ProvekIt mints the proof. Mint composes lift and lower. The npm lifter handles package rails, delegates the contract surface to the TypeScript lifter, receives lifted ProofIR contracts from contracts.ts, and adds the witness demands. The JavaScript/package-manifest lowerer emits witness proofs for each demanded contract. The main .proof gets a root CID only because those witness inputs exist.

What to look for: package name and version are ordinary identity. contractSetCid and proofFile are admission receipts. The baseline is green because the witness lowerer can emit evidence for runtime.no-env-secret-read.
TEXT

print_provekit package inspect "$baseline_rel" --json --quiet
run_provekit_capture "$inspect_json" "$inspect_stderr" package inspect "$baseline_rel" --json --quiet

section "Baseline Package Inspection JSON"
show_json_file "$inspect_json"
highlight_raw_line "$inspect_json" '"name": "safe-json"' "The package identity is the expected npm package."
highlight_raw_line "$inspect_json" '"version": "1.4.1"' "This is the admitted baseline release."
highlight_raw_line "$inspect_json" '"lifter": "provekit-lift-ts"' "The contract rail is lifted from TypeScript, not hand-authored package JSON."
highlight_raw_line "$inspect_json" '"inputClosureCid":' "ProvekIt computes the CI input-closure rail from package inputs."

print_provekit mint --project "$work_pkg" --out "$work_pkg" --no-attest --json --quiet
run_provekit_capture "$mint_json" "$mint_stderr" mint --project "$work_pkg" --out "$work_pkg" --no-attest --json --quiet

section "Baseline Mint JSON"
show_json_file "$mint_json"
highlight_raw_line "$mint_json" '"ok": true,' "The full lift/lower/mint chain admitted the baseline."
highlight_raw_line "$mint_json" '"filenameCid":' "The .proof file itself has a content-derived identity."
highlight_raw_line "$mint_json" '"contractSetCid":' "The main proof records a content-addressed contract set."
highlight_raw_line "$mint_json" '"proofFile":' "The admitted baseline has a concrete .proof artifact."

print_provekit package inspect "$work_pkg" --json --quiet
run_provekit_capture "$post_proof_inspect_json" "$post_proof_inspect_stderr" package inspect "$work_pkg" --json --quiet

section "Baseline Package Inspection After .proof Mint"
show_json_file "$post_proof_inspect_json"
highlight_raw_line "$post_proof_inspect_json" '"proofs": {' "The package inspector treats shipped .proof files as package artifacts, not side-channel prose."
highlight_raw_line "$post_proof_inspect_json" '"contentCid":' "The .proof bytes are addressable by BLAKE3-512 like the package tarball and contract set."
highlight_raw_line "$post_proof_inspect_json" '"filenameMatchesContent": true' "The generated filename is the same CID as the .proof content."

analysis_with_receipts <<'TEXT'
The first package inspection proves ProvekIt is looking at the npm-shaped release inputs and using the TypeScript lifter for the contract surface. The mint lines prove the baseline was not accepted by identity alone: a main .proof was minted only after the lifted contracts and witness demands composed.

The second package inspection is the supply-chain move. After minting, the `.proof` itself is just another content-addressed artifact in the package rail map. The tarball has a binaryCid, the input closure has an inputClosureCid, the contract set has a contractSetCid, and the proof file has a contentCid.

This is the green side of green-red. The baseline gives the later update something concrete to preserve.
TEXT

next_script "03-show-conventional-green.sh"
