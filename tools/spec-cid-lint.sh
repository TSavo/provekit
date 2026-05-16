#!/usr/bin/env bash
# spec-cid-lint.sh: enforce that every blake3-512 CID literal in protocol/specs/
# is either a visible abbreviation or exactly 128 hex chars.
#
# A "visible abbreviation" is `blake3-512:<short-hex>` followed immediately by
# `...` (or unicode ellipsis), OR `<hex>` of length <= 16 (compact placeholders
# like `blake3-512:1111`).
#
# Catches the LLM-hand-typing-CIDs failure mode: visual patterns like
# "1a1a..." that get typed with 65 or 66 pair repetitions instead of 64,
# producing 130 or 132 hex chars where 128 is required. Such CIDs would
# fail the spec's own malformed-cid relift check, which makes the spec
# self-contradictory.
#
# A real blake3-512 hash is 64 bytes = 128 hex chars. The CDDL grammar in
# every comment-sugar spec says `"blake3-512:" 128HEXDIG`.
#
# Exit 0: clean. Exit 1: at least one malformed CID literal. Output the
# file:line locations and offending lengths.

set -uo pipefail

ROOT="${1:-protocol/specs}"

# grep -P for Perl regex; -o emits each match with offset; -n adds line number.
# Match: `blake3-512:` + 1+ hex + (NOT followed by . OR ... marker).
# A CID is "abbreviated" if it has <=16 hex AND is followed by anything,
# OR if it is followed by a `.` (the start of `...`).

bad=0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  # Strip grep's file:line: prefix to get the captured fragment.
  fragment=$(printf '%s' "$line" | sed -E 's/^[^:]+:[0-9]+://')
  hex=$(printf '%s' "$fragment" | sed -E 's/^blake3-512:([0-9a-f]+).*/\1/')
  trailing=$(printf '%s' "$fragment" | sed -E 's/^blake3-512:[0-9a-f]+//')
  len=${#hex}
  # Tolerate short visible placeholders (<=16 hex) regardless of trailing.
  if [ "$len" -le 16 ]; then
    continue
  fi
  # Tolerate any length followed immediately by an ASCII `.` (start of `...`)
  # or a unicode ellipsis.
  case "$trailing" in
    "."*|"…"*) continue ;;
  esac
  # Tolerate exactly 128 hex chars (the only valid blake3-512 form).
  if [ "$len" -eq 128 ]; then
    continue
  fi
  printf 'malformed CID (length=%d, expected 128 hex chars or <=16 + ellipsis): %s\n' "$len" "$line" >&2
  bad=$((bad + 1))
done < <(grep -rnoE 'blake3-512:[0-9a-f]+\.{0,3}' "$ROOT" 2>/dev/null || true)

if [ "$bad" -gt 0 ]; then
  printf '\nspec-cid-lint: %d malformed blake3-512 CID literal(s) under %s\n' "$bad" "$ROOT" >&2
  printf 'A blake3-512 hash is 64 bytes (128 hex chars). Abbreviations <=16 hex or hex+`...` are tolerated.\n' >&2
  exit 1
fi

exit 0
