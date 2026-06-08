#!/usr/bin/env bash

set -u

failures=0
tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

SMT_INPUT='(set-logic ALL)(assert (distinct 1 1))(check-sat)'

pass() {
  printf 'PASS %s\n' "$1"
}

fail() {
  printf 'FAIL %s: %s\n' "$1" "$2"
  failures=$((failures + 1))
}

skip() {
  printf 'SKIP %s: %s\n' "$1" "$2"
}

need_binary() {
  command -v "$1" >/dev/null 2>&1
}

expect_unsat() {
  printf '%s\n' "$1" | grep -Eq '(^|[[:space:]])unsat($|[[:space:]])'
}

verify_z3() {
  local out
  if ! need_binary z3; then
    fail z3 "z3 not on PATH"
    return
  fi
  if ! out="$(printf '%s\n' "$SMT_INPUT" | z3 -in 2>&1)"; then
    fail z3 "$out"
    return
  fi
  if expect_unsat "$out"; then
    pass z3
  else
    fail z3 "expected unsat, got: $out"
  fi
}

verify_cvc5() {
  local out
  if ! need_binary cvc5; then
    fail cvc5 "cvc5 not on PATH"
    return
  fi
  if ! out="$(printf '%s\n' "$SMT_INPUT" | cvc5 --lang=smt2 2>&1)"; then
    fail cvc5 "$out"
    return
  fi
  if expect_unsat "$out"; then
    pass cvc5
  else
    fail cvc5 "expected unsat, got: $out"
  fi
}

verify_vampire() {
  local out
  if ! need_binary vampire; then
    fail vampire "vampire not on PATH"
    return
  fi
  if ! out="$(printf '%s\n' "$SMT_INPUT" | vampire --input_syntax smtlib2 --output_mode smtcomp 2>&1)"; then
    fail vampire "$out"
    return
  fi
  if expect_unsat "$out"; then
    pass vampire
  else
    fail vampire "expected unsat, got: $out"
  fi
}

verify_coqc() {
  local dir out
  if ! need_binary coqc; then
    fail coqc "coqc not on PATH"
    return
  fi
  dir="$tmp_root/coq"
  mkdir -p "$dir"
  cat > "$dir/t.v" <<'COQ'
Lemma triv : 1 = 1.
Proof.
  reflexivity.
Qed.
COQ
  if out="$(cd "$dir" && coqc t.v 2>&1)"; then
    pass coqc
  else
    fail coqc "$out"
  fi
}

verify_maude() {
  local dir out
  if ! need_binary maude; then
    fail maude "maude not on PATH"
    return
  fi
  dir="$tmp_root/maude"
  mkdir -p "$dir"
  cat > "$dir/t.maude" <<'MAUDE'
fmod SUGAR-SMOKE is
  sort Nat .
  op 0 : -> Nat .
  op s : Nat -> Nat .
  op _+_ : Nat Nat -> Nat .
  vars N M : Nat .
  eq 0 + M = M .
  eq s(N) + M = s(N + M) .
endfm

reduce in SUGAR-SMOKE : 0 + 0 .
MAUDE
  if ! out="$(maude "$dir/t.maude" 2>&1)"; then
    fail maude "$out"
    return
  fi
  if printf '%s\n' "$out" | grep -Eq 'result Nat:[[:space:]]*0'; then
    pass maude
  else
    fail maude "expected normal form 0, got: $out"
  fi
}

extract_cpf() {
  local raw="$1"
  local cert="$2"
  awk 'BEGIN { emit = 0 } /^<\?xml/ || /^<certificationProblem/ || /^<cpf/ { emit = 1 } emit { print }' "$raw" > "$cert"
  if [ ! -s "$cert" ]; then
    cp "$raw" "$cert"
  fi
}

verify_aprove_ceta() {
  local dir out ceta_out
  if ! need_binary aprove || ! need_binary ceta; then
    skip aprove-ceta "aprove and/or ceta not on PATH; the Maude/CeTA gate runs in untrusted mode (portfolio falls through to vampire/coq for those obligations). Install aprove + ceta to enable it."
    return
  fi
  dir="$tmp_root/aprove-ceta"
  mkdir -p "$dir"
  cat > "$dir/system.trs" <<'TRS'
(VAR x)
(RULES
  f(x) -> x
)
TRS
  if ! out="$(aprove -m wst -p cpf -C ceta "$dir/system.trs" > "$dir/raw.cpf" 2>&1)"; then
    fail aprove-ceta "$out"
    return
  fi
  extract_cpf "$dir/raw.cpf" "$dir/cert.cpf"
  if ! ceta_out="$(ceta "$dir/cert.cpf" 2>&1)"; then
    fail aprove-ceta "$ceta_out"
    return
  fi
  if printf '%s\n' "$ceta_out" | grep -Eiq 'YES|CERTIFIED|accepted|proof accepted|certificate accepted'; then
    pass aprove-ceta
  else
    fail aprove-ceta "CeTA did not accept certificate: $ceta_out"
  fi
}

verify_csi() {
  local dir out
  if ! need_binary csi; then
    skip csi "csi not on PATH; the Maude/CeTA gate's confluence check is unavailable (gate runs in untrusted mode). Install csi to enable it."
    return
  fi
  dir="$tmp_root/csi"
  mkdir -p "$dir"
  cat > "$dir/system.trs" <<'TRS'
(VAR x)
(RULES
  f(x) -> x
)
TRS
  if ! out="$(csi "$dir/system.trs" 2>&1)"; then
    fail csi "$out"
    return
  fi
  if printf '%s\n' "$out" | grep -Eq '(^|[[:space:]])YES($|[[:space:]])'; then
    pass csi
  else
    fail csi "expected YES, got: $out"
  fi
}

verify_lean() {
  local project dir out
  if ! need_binary lake; then
    fail lean "lake not on PATH"
    return
  fi
  if ! need_binary lean; then
    fail lean "lean not on PATH"
    return
  fi
  project="${SUGAR_LEAN_PROJECT:-/opt/lean-mathlib}"
  if [ ! -f "$project/lakefile.lean" ]; then
    fail lean "lake project not found at $project"
    return
  fi
  dir="$tmp_root/lean"
  mkdir -p "$dir"
  cat > "$dir/Triv.lean" <<'LEAN'
import Mathlib

theorem triv : (1 : Nat) = 1 := rfl

#print axioms triv
LEAN
  if ! out="$(cd "$project" && lake env lean "$dir/Triv.lean" 2>&1)"; then
    fail lean "$out"
    return
  fi
  if printf '%s\n' "$out" | grep -q 'sorryAx'; then
    fail lean "proof depends on sorryAx: $out"
  else
    pass lean
  fi
}

verify_z3
verify_cvc5
verify_vampire
verify_coqc
verify_maude
verify_aprove_ceta
verify_csi
verify_lean

if [ "$failures" -eq 0 ]; then
  exit 0
fi

printf 'FAIL portfolio: %s backend checks failed\n' "$failures"
exit 1
