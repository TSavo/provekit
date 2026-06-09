#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HOOK="$ROOT/hooks/pre-commit"

if [ ! -x "$HOOK" ]; then
  echo "missing executable hook: $HOOK" >&2
  exit 1
fi

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

write_minimal_repo() {
  local repo="$1"
  mkdir -p "$repo/implementations/rust/src" "$repo/hooks"
  cp "$HOOK" "$repo/hooks/pre-commit"
  chmod +x "$repo/hooks/pre-commit"
  cat > "$repo/implementations/rust/Cargo.toml" <<'TOML'
[package]
name = "fmt-hook-fixture"
version = "0.1.0"
edition = "2021"
TOML
  cat > "$repo/implementations/rust/src/lib.rs" <<'RS'
pub fn fixture() -> i32 {
    0
}
RS
  (
    cd "$repo"
    git init -q
    git config user.name "Hook Test"
    git config user.email "hook-test@example.com"
    git add implementations/rust/Cargo.toml implementations/rust/src/lib.rs
    git -c commit.gpgsign=false commit -q -m init
    git config core.hooksPath hooks
  )
}

expect_blob() {
  local repo="$1"
  local path="$2"
  local expected="$3"
  local actual="$TMPDIR/blob"
  (
    cd "$repo"
    git show "HEAD:$path" > "$actual"
  )
  printf '%s' "$expected" | diff -u - "$actual"
}

commit_in_repo() {
  local repo="$1"
  local message="$2"
  shift 2
  (
    cd "$repo"
    git -c commit.gpgsign=false commit -m "$message" "$@"
  )
}

repo="$TMPDIR/repo"
write_minimal_repo "$repo"

(
  cd "$repo"
  cat > sample.rs <<'RS'
pub fn sample( )->i32{1}
RS
  git add sample.rs
)
commit_in_repo "$repo" "format staged rust" >/tmp/pre-commit-format-sample.out 2>&1
expect_blob "$repo" sample.rs $'pub fn sample() -> i32 {\n    1\n}\n'

(
  cd "$repo"
  cat > clean.rs <<'RS'
pub fn clean() -> i32 {
    2
}
RS
  git add clean.rs
)
commit_in_repo "$repo" "clean rust passes" >/tmp/pre-commit-format-clean.out 2>&1
expect_blob "$repo" clean.rs $'pub fn clean() -> i32 {\n    2\n}\n'

(
  cd "$repo"
  cat > unstaged.rs <<'RS'
pub fn unstaged( )->i32{3}
RS
  cat > README.md <<'MD'
notes
MD
  git add README.md
)
commit_in_repo "$repo" "no staged rust no op" >/tmp/pre-commit-format-norust.out 2>&1
grep -q 'pub fn unstaged( )->i32{3}' "$repo/unstaged.rs"

(
  cd "$repo"
  cat > partial.rs <<'RS'
pub fn staged() -> i32 {
    1
}

pub fn keep() -> i32 {
    2
}
RS
  git add partial.rs
  git -c commit.gpgsign=false commit -q -m "partial base"

  cat > partial.rs <<'RS'
pub fn staged( )->i32{1}

pub fn keep() -> i32 {
    2
}
RS
  git add partial.rs
  cat > partial.rs <<'RS'
pub fn staged( )->i32{1}

pub fn keep( )->i32{2}
RS
)
commit_in_repo "$repo" "skip partial staging" >/tmp/pre-commit-format-partial.out 2>&1
grep -q "skipping partially staged Rust file: partial.rs" /tmp/pre-commit-format-partial.out
expect_blob "$repo" partial.rs $'pub fn staged( )->i32{1}\n\npub fn keep() -> i32 {\n    2\n}\n'
grep -q 'pub fn keep( )->i32{2}' "$repo/partial.rs"

echo "pre-commit format hook tests passed"
