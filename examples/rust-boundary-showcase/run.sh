#!/usr/bin/env bash
# The rust by-reference boundary showcase. The two-way mirror of numpy-showcase,
# but for RUST source (not a python vendor):
#
#   derive-lift — the vendor crate's PLAIN `pub fn reverse_chars` (NO
#                 `#[provekit::sugar]` tag) is lifted with PROVEKIT_LEAN_SOURCE=1
#                 into a LEAN .proof (locus + source_cid/template_cid, NO inline
#                 body) staged into the consumer's .provekit/imports/. In the
#                 library-bindings layer the binding is DERIVED from the crate
#                 name + fn name — write a function, it's sugar; the tag is gone.
#                 The body lives only on vendor disk; the SourceMemento points
#                 at it.
#   mint        — the lean binding is sealed into a content-addressed .proof.
#   materialize — the consumer's `#[provekit::boundary(library, call)]` stub gets
#                 its body filled with reverse_chars's REAL source, resolved by
#                 the Source Oracle from the live vendor crate and CID-verified
#                 against the frozen pin.
#   DRIFT       — tamper the vendor source AFTER the mint; the frozen pin no
#                 longer matches live disk, so materialize REFUSES (no write).
#
# Everything is kit-side; the .proof is the transport; rust stays proof-blind.
# NOTE: no `set -e` — the drift leg intentionally REFUSES (non-zero verb exit);
# this script captures both verdicts and PASSES iff provekit produces exactly
# them (fill succeeds + body matches; drift refused). Mirrors numpy-showcase.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
BIN="$REPO/implementations/rust/target/debug/provekit"
WALK="$REPO/implementations/rust/target/debug/provekit-walk-rpc"

VENDOR="$HERE/vendor"
CONSUMER="$HERE/consumer"

if [ ! -x "$BIN" ] || [ ! -x "$WALK" ]; then
  echo "building provekit + walk-rpc ..."
  ( cd "$REPO/implementations/rust" && cargo build -p sugar-cli --bin provekit -p sugar-walk --bin provekit-walk-rpc ) || {
    echo "FAIL: cargo build failed"; exit 1; }
fi

# --- restore both source files to their pristine (pre-tamper) state ----------
cat > "$VENDOR/src/lib.rs" <<'RS'
// The vendor's REAL source — PLAIN rust, no provekit attribute of any kind.
// `reverse_chars` is an ordinary `pub fn`. In the `library-bindings` layer the
// lift DERIVES the binding from the crate name + fn name (no
// `#[provekit::sugar]` required): write a function, it's sugar. Lifting it with
// PROVEKIT_LEAN_SOURCE=1 mints a LEAN binding (locus + source_cid/template_cid,
// NO inline body). The body lives ONLY here on disk; the Source Oracle resolves
// it on demand IFF this source recomputes to the pinned CIDs.
//
// run.sh's DRIFT leg tampers this body after the mint — the pin then no longer
// matches disk, and materialize REFUSES. Keep this body in sync with run.sh's
// restore block (it rewrites this file before each run).

pub fn reverse_chars(s: &str) -> String {
    s.chars().rev().collect()
}
RS

cat > "$CONSUMER/src/lib.rs" <<'RS'
// The consumer. `rev` is a `#[provekit::boundary]` stub: its body is a
// placeholder until `materialize-boundary` fills it from the vendor's REAL
// `reverse_chars` source (CID-verified against the frozen vendor .proof).
#[provekit::boundary(concept = "concept:reverse-string", library = "rust-boundary-vendor", call = "reverse_chars")]
pub fn rev(s: &str) -> String {
    unimplemented!("materialize-fillable boundary")
}
RS

# Capture the pristine consumer body (with the filled body) for the post-fill
# body-match check below.
EXPECTED_BODY="s.chars().rev().collect()"

# --- lift manifests (interpolate the built walk-rpc path + lean env) ----------
mkdir -p "$VENDOR/.provekit/lift/rust-bind" "$CONSUMER/.provekit/lift/rust-bind" "$CONSUMER/.provekit/imports"
# Vendor: lean lift (PROVEKIT_LEAN_SOURCE=1 in command) -> lean .proof at mint.
cat > "$VENDOR/.provekit/lift/rust-bind/manifest.toml" <<EOF
name = "rust-bind-lift"
kind = "lift"
command = ["/usr/bin/env", "PROVEKIT_LEAN_SOURCE=1", "$WALK", "--rpc"]
working_dir = "."
[capabilities]
authoring_surfaces = ["rust-bind"]
EOF
# Consumer: serves provekit.plugin.materialize (no lean env needed; lean only
# affects the mint path, not the materialize resolution).
cat > "$CONSUMER/.provekit/config.toml" <<EOF
[authoring]
surface = "rust-bind"
[[plugins]]
name = "rust-sugar"
surface = "rust-bind"
layer = "library-bindings"
EOF
cat > "$CONSUMER/.provekit/lift/rust-bind/manifest.toml" <<EOF
name = "rust-bind-lift"
kind = "lift"
command = ["$WALK", "--rpc"]
working_dir = "."
[capabilities]
authoring_surfaces = ["rust-bind"]
EOF

# --- clean prior artifacts ---------------------------------------------------
rm -f "$CONSUMER/.provekit/imports/"*.proof 2>/dev/null || true

echo "== derive-lift vendor -> consumer/.provekit/imports/ (lean: CIDs, not inline body) =="
"$BIN" mint --project "$VENDOR" --out "$CONSUMER/.provekit/imports" --library-bindings --quiet
VENDOR_PROOF="$(ls "$CONSUMER/.provekit/imports/"*.proof 2>/dev/null | head -1)"
if [ -z "$VENDOR_PROOF" ]; then echo "FAIL: no vendor .proof minted"; exit 1; fi
echo "  vendor derived .proof: $(basename "$VENDOR_PROOF")"

fail=0

echo ""
echo "== materialize @boundary(rust-boundary-vendor.reverse_chars) (body resolved by the oracle) =="
FILL_JSON="$("$BIN" materialize-boundary --project "$CONSUMER" --source src/lib.rs --vendor-root "$VENDOR" --json 2>/dev/null)"
FILL_CODE=$?
echo "$FILL_JSON" | sed 's/^/  /'

echo ""
echo "== self-check 1: the FILL succeeded and the stub body == reverse_chars's REAL body =="
# verb exit 0 (no refusal) AND outcome materialized AND the rewritten file holds
# the vendor body.
"$BIN" materialize-boundary --project "$CONSUMER" --source src/lib.rs --vendor-root "$VENDOR" --write >/dev/null 2>&1
fill_status=$?
if [ "$fill_status" -eq 0 ]; then echo "  ok: materialize-boundary exited 0 (filled, no refusal)"; else echo "  FAIL: materialize-boundary exited $fill_status on the clean fill"; fail=1; fi
if grep -qF "$EXPECTED_BODY" "$CONSUMER/src/lib.rs"; then
  echo "  ok: consumer stub body now contains the vendor body \`$EXPECTED_BODY\`"
else
  echo "  FAIL: consumer body does not contain the resolved vendor body"; fail=1
  echo "  --- consumer/src/lib.rs ---"; sed 's/^/    /' "$CONSUMER/src/lib.rs"
fi
# The filled file must still parse + no longer contain the unimplemented stub.
if grep -q "unimplemented" "$CONSUMER/src/lib.rs"; then
  echo "  FAIL: filled file still contains the unimplemented stub"; fail=1
else
  echo "  ok: the unimplemented stub is gone (body materialized)"
fi

echo ""
echo "== DRIFT: tamper the vendor source AFTER the mint; the oracle must REFUSE =="
# Restore the consumer stub so there is a boundary to fill again.
cat > "$CONSUMER/src/lib.rs" <<'RS'
#[provekit::boundary(concept = "concept:reverse-string", library = "rust-boundary-vendor", call = "reverse_chars")]
pub fn rev(s: &str) -> String {
    unimplemented!("materialize-fillable boundary")
}
RS
# Tamper the vendor body (same behavior, different source bytes -> source_cid
# drifts from the frozen pin). Still PLAIN source — no tag; drift is detected
# by the CID, not by any annotation.
cat > "$VENDOR/src/lib.rs" <<'RS'
pub fn reverse_chars(s: &str) -> String {
    let collected: Vec<char> = s.chars().rev().collect();
    collected.into_iter().collect()
}
RS
DRIFT_JSON="$("$BIN" materialize-boundary --project "$CONSUMER" --source src/lib.rs --vendor-root "$VENDOR" --write --json 2>/dev/null)"
drift_status=$?
echo "$DRIFT_JSON" | sed 's/^/  /'

echo ""
echo "== self-check 2: the DRIFT was REFUSED (non-zero verb exit, outcome refused, no write) =="
if [ "$drift_status" -ne 0 ]; then echo "  ok: materialize-boundary exited non-zero on drift ($drift_status)"; else echo "  FAIL: drift returned exit 0 (should refuse)"; fail=1; fi
if echo "$DRIFT_JSON" | grep -q '"outcome": *"refused"'; then
  echo "  ok: outcome is refused"
else
  echo "  FAIL: drift outcome was not refused"; fail=1
fi
if echo "$DRIFT_JSON" | grep -q "CID misaligned"; then
  echo "  ok: refusal cites a CID misalignment (source drifted from the proof)"
else
  echo "  FAIL: refusal did not cite a CID misalignment"; fail=1
fi
# The refused stub must NOT have been written — it still holds the unimplemented body.
if grep -q "unimplemented" "$CONSUMER/src/lib.rs"; then
  echo "  ok: the consumer stub was NOT rewritten on refusal (still unimplemented)"
else
  echo "  FAIL: the stub was rewritten despite the refusal"; fail=1
fi

echo ""
if [ "$fail" -eq 0 ]; then
  echo "PASS: rust by-reference boundary chain — clean fill matched the vendor body; drift refused."
else
  echo "FAIL: provekit did not produce the expected verdict."; exit 1
fi
