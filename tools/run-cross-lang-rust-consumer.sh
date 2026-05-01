#!/bin/sh
# Build + run the Rust cross-language consumer demo:
#   Peer (C++ and/or Go) publishes; Rust verifies; Rust catches parse_int(num(0)).
#
# Walks every peer-language publisher available on this machine and
# feeds each one's .proof to the Rust consumer in turn.
#
# Prereqs:
#   - z3 installed and on PATH (or PROVEKIT_Z3 set)
#   - cargo / rustc on PATH (the Rust workspace builds the example)
#   - For the Go publisher cell: go on PATH
#   - For the C++ publisher cell: a pre-built /tmp/cpp-kit-out-v11/*.proof
#     produced by the C++ kit (mirrors run-cross-lang-consumer.sh's prereqs)
#
# Usage:
#   tools/run-cross-lang-rust-consumer.sh

set -e

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
RUST_DIR="$WORKSPACE/implementations/rust"
GO_PUBLISH="$WORKSPACE/implementations/go/provekit-ir-symbolic"
GO_OUT_DIR="${GO_OUT_DIR:-/tmp/go-kit-out-v11}"
CPP_OUT_DIR="${CPP_OUT_DIR:-/tmp/cpp-kit-out-v11}"

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not on PATH; aborting" >&2
    exit 2
fi

# ---- 1a. Try to refresh the Go publisher (skipped if go missing) ----
GO_PROOF=""
if [ -d "$GO_PUBLISH" ] && command -v go >/dev/null 2>&1; then
    mkdir -p "$GO_OUT_DIR"
    if ( cd "$GO_PUBLISH" && go run ./cmd/go-kit-publish "$GO_OUT_DIR" >/dev/null ); then
        GO_PROOF="$(ls "$GO_OUT_DIR"/*.proof 2>/dev/null | head -1)"
    fi
fi

# ---- 1b. Discover the C++ .proof (assumed already published) ----
CPP_PROOF=""
if [ -d "$CPP_OUT_DIR" ]; then
    CPP_PROOF="$(ls "$CPP_OUT_DIR"/*.proof 2>/dev/null | head -1)"
fi

# ---- 2. Build the Rust consumer once ----
( cd "$RUST_DIR" && cargo build --release --example cross_lang_consume )

# ---- 3. Run against every peer .proof we managed to find ----
RC=0
if [ -n "$CPP_PROOF" ]; then
    echo "==== Cell cpp -> rust ===="
    ( cd "$RUST_DIR" && cargo run --release --example cross_lang_consume -- "$CPP_PROOF" ) || RC=$?
else
    echo "No C++ .proof in $CPP_OUT_DIR; skipping cpp -> rust cell." >&2
fi
if [ -n "$GO_PROOF" ]; then
    echo "==== Cell go -> rust ===="
    ( cd "$RUST_DIR" && cargo run --release --example cross_lang_consume -- "$GO_PROOF" ) || RC=$?
else
    echo "No Go .proof in $GO_OUT_DIR; skipping go -> rust cell." >&2
fi

if [ -z "$CPP_PROOF" ] && [ -z "$GO_PROOF" ]; then
    echo "No peer .proof available; nothing to verify." >&2
    exit 1
fi
exit "$RC"
