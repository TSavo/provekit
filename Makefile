# ProvekIt — top-level orchestrator
#
# Five-language polyglot. Each language owns its native build tool;
# this Makefile is glue, not a build system. `make ci` runs the same
# gate the GitHub Actions workflow runs.
#
# Mainline targets:
#   make help        — print this help
#   make ci          — full conformance gate (catalog + protocol + 5 mints + tests)
#   make conformance — catalog + protocol + 5 mint CIDs match pinned values
#   make all-mint    — run all 5 mint commands; print CIDs
#   make test-all    — run every language-native test suite
#
# Per-language targets:
#   make build-rust  — cargo build --release for workspace + tools
#   make build-cpp   — vendored-blake3 clang++ build of the C++ orchestrator
#   make test-rust / test-go / test-ts / test-csharp / test-python
#
# Determinism:
#   make ci is the contract. If it's green, every peer's self-contracts
#   round-trip to its pinned CID, the catalog v1.2.0 hash matches, and
#   every native test suite passes. Anything else is decoration.

.DEFAULT_GOAL := help

# --- Pinned CIDs ------------------------------------------------------------
#
# Bumping a self-contracts CID is now an explicit attestation event, NOT a
# Makefile string edit:
#
#   1. Make your code change in `implementations/<lang>/provekit-self-contracts`
#      (or the Go / C++ analog).
#   2. `make build-rust && make mint-rust`   (or mint-go / mint-cpp)
#      -> the mint target FAILS and prints the new CID.
#   3. `cargo run --release --manifest-path tools/foundation-keygen/Cargo.toml \
#         --bin sign-self-contracts -- <lang> <new-cid>`
#      -> rewrites `.provekit/self-contracts-attestations/<lang>.json` with
#         a fresh foundation-v0 ed25519 signature over the new CID.
#   4. `git add .provekit/self-contracts-attestations/<lang>.json && git commit`
#
# The bundle (letter) does not know its own CID. The on-disk attestation
# (envelope) names the CID and is signed externally. See
# `protocol/specs/2026-05-02-binary-attestation-protocol.md` for the
# letter-envelope framing and `protocol/specs/2026-05-02-provekit-migrate-protocol.md`
# for the documented bump dance.
#
# `CATALOG_CID` is bumped to v1.3.1 here; the constant remains because
# `make help` echoes it. Follow-up: retire it the same way the
# self-contracts CIDs are retired (read from the embedded catalog
# signature attestation).
CATALOG_CID := blake3-512:dab2eca97eaea7cc107b1ff3f2326094d804a5e91749bf8e9caa36cd049dc0ae1cb65afb353af8fcd271f87e9e0fc7e7710ec6a68666da6a11f802bc304ff799

# `TS_CID` and `CSHARP_CID` retain the old self-reference pattern for now;
# follow-up: extend the letter-envelope refactor to those two peers.
TS_CID      := blake3-512:449339930add6457bf25542f2117a025daada4a4bd1de704737750ad6d1c1be814c284d31bb97159ca0b2d2c52f8c043a64533d3432195f5a0f338c5d4904d44
CSHARP_CID  := blake3-512:cec85197e5bc394cb97fa3b96c076eca5ace3eeda819f8a2b8b7001f85336dbfadc7e28be3a38676f81387f908b327f0fffeae7d6d04fe76a8c754e5db38c61e

PROVEKIT := implementations/rust/target/release/provekit
VERIFY_SELF_CONTRACTS := tools/foundation-keygen/target/release/verify-self-contracts
SELF_CONTRACTS_ATTEST_DIR := .provekit/self-contracts-attestations

.PHONY: help
help:
	@echo "ProvekIt — top-level orchestrator"
	@echo ""
	@echo "Mainline:"
	@echo "  make ci             full gate (conformance + test-all)"
	@echo "  make conformance    catalog + protocol + 5 mint CIDs match pinned"
	@echo "  make all-mint       run all 5 mint commands; print CIDs"
	@echo "  make test-all       run all language-native test suites"
	@echo ""
	@echo "Per-language build:"
	@echo "  make build-rust     cargo build --release (workspace + tools)"
	@echo "  make build-cpp      clang++ + vendored-blake3"
	@echo "  make build-go       go build per Go module"
	@echo "  make build-ts       pnpm install"
	@echo "  make build-csharp   dotnet build"
	@echo ""
	@echo "Per-language test:"
	@echo "  make test-rust  test-go  test-cpp  test-ts  test-csharp  test-python"
	@echo ""
	@echo "Self-lift experiments:"
	@echo "  make self-lift-canonicalizer  run provekit-lift against the canonicalizer crate"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean          remove build artifacts"
	@echo ""
	@echo "Pinned CIDs (catalog v1.3.1):"
	@echo "  catalog: $(CATALOG_CID)"
	@echo "  rust:    (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/rust.json"
	@echo "  go:      (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/go.json"
	@echo "  cpp:     (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json"
	@echo "  ts:      $(TS_CID)"
	@echo "  csharp:  $(CSHARP_CID)"

# --- Per-language builds -----------------------------------------------------

.PHONY: build-rust
build-rust:
	cargo build --release --manifest-path implementations/rust/Cargo.toml
	cargo build --release --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo build --release --manifest-path tools/foundation-keygen/Cargo.toml

.PHONY: build-cpp
build-cpp:
	tools/build-cpp-self-contracts.sh --build-only

.PHONY: build-go
build-go:
	cd implementations/go/provekit-ir-symbolic && go build ./...
	cd implementations/go/provekit-self-contracts && go build ./...
	cd implementations/go/provekit-lift-go-tests && go build ./...

.PHONY: build-ts
build-ts:
	pnpm install --frozen-lockfile

.PHONY: build-csharp
build-csharp:
	dotnet build implementations/csharp/Provekit.sln --configuration Release --nologo

# --- Mint targets ------------------------------------------------------------

# Each mint target builds its peer + dispatches via `provekit mint`,
# then asserts the printed CID equals the pinned value. CI uses these.

.PHONY: mint-rust
mint-rust: build-rust
	@echo ">> minting rust self-contracts"
	@out=$$($(PROVEKIT) mint --project implementations/rust --quiet); \
	echo "  cid: $$out"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/rust.json "$$out" || \
		(echo "FAIL: rust self-contracts attestation rejected; bump dance:" && \
		 echo "      cargo run --release --manifest-path tools/foundation-keygen/Cargo.toml \\\\" && \
		 echo "        --bin sign-self-contracts -- rust $$out" && exit 1)

.PHONY: mint-go
mint-go: build-rust build-go
	@echo ">> minting go self-contracts"
	@out=$$($(PROVEKIT) mint --project implementations/go --quiet); \
	echo "  cid: $$out"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/go.json "$$out" || \
		(echo "FAIL: go self-contracts attestation rejected; bump dance:" && \
		 echo "      cargo run --release --manifest-path tools/foundation-keygen/Cargo.toml \\\\" && \
		 echo "        --bin sign-self-contracts -- go $$out" && exit 1)

.PHONY: mint-cpp
mint-cpp: build-rust build-cpp
	@echo ">> minting cpp self-contracts"
	@out=$$($(PROVEKIT) mint --project implementations/cpp --quiet); \
	echo "  cid: $$out"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json "$$out" || \
		(echo "FAIL: cpp self-contracts attestation rejected; bump dance:" && \
		 echo "      cargo run --release --manifest-path tools/foundation-keygen/Cargo.toml \\\\" && \
		 echo "        --bin sign-self-contracts -- cpp $$out" && exit 1)

.PHONY: mint-ts
mint-ts: build-ts
	@echo ">> minting ts self-contracts (vitest path)"
	pnpm vitest run implementations/typescript/src/bin/mint-ts-self-contracts.test.ts

.PHONY: mint-csharp
mint-csharp:
	@echo ">> minting csharp self-contracts"
	@out=$$(cd implementations/csharp/Provekit.SelfContracts && \
		dotnet run -c Release 2>/dev/null \
		| grep -F 'catalog CID:' | awk '{print $$NF}' | head -1); \
	echo "  cid: $$out"; \
	test "$$out" = "$(CSHARP_CID)" || \
		(echo "FAIL: csharp CID mismatch (expected $(CSHARP_CID))" && exit 1)

.PHONY: all-mint
all-mint: mint-rust mint-go mint-cpp mint-ts mint-csharp
	@echo ""
	@echo "==== all 5 self-contract CIDs match pinned values ===="
	@printf "  %-8s  %s\n" "rust"   "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/rust.json)"
	@printf "  %-8s  %s\n" "go"     "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/go.json)"
	@printf "  %-8s  %s\n" "cpp"    "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json)"
	@printf "  %-8s  %s\n" "ts"     "$(TS_CID)"
	@printf "  %-8s  %s\n" "csharp" "$(CSHARP_CID)"

# --- Conformance gate --------------------------------------------------------

# Default invocation (no args) is read-only since audit #180 fix; --verify
# is now a no-op alias retained because protocol-catalog-format.md §5
# names it literally. Either form is safe; only --write mutates the catalog.
.PHONY: catalog-verify
catalog-verify:
	cargo run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify

.PHONY: protocol-verify
protocol-verify: build-rust
	$(PROVEKIT) verify-protocol --signed

.PHONY: conformance
conformance: catalog-verify protocol-verify all-mint
	@echo ""
	@echo "==== conformance: PASS ===="

# --- Per-language test suites ------------------------------------------------

.PHONY: test-rust
test-rust:
	cargo test --release --manifest-path implementations/rust/Cargo.toml
	cargo test --release --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo test --release --manifest-path tools/foundation-keygen/Cargo.toml

.PHONY: test-go
test-go:
	cd implementations/go/provekit-ir-symbolic && go test ./...
	cd implementations/go/provekit-self-contracts && go test ./...
	cd implementations/go/provekit-lift-go-tests && go test ./...

.PHONY: test-cpp
test-cpp: build-cpp
	@echo "test-cpp: cpp suite is the mint round-trip; covered by mint-cpp"

.PHONY: test-ts
test-ts:
	pnpm test

.PHONY: test-csharp
test-csharp:
	dotnet test implementations/csharp/Provekit.sln --nologo --verbosity quiet

.PHONY: test-python
test-python:
	cd implementations/python/provekit-lift-py-tests && \
		pip install --quiet -e . && \
		pip install --quiet pytest && \
		pytest

.PHONY: test-all
test-all: test-rust test-go test-ts test-csharp test-python
	@echo ""
	@echo "==== test-all: PASS ===="

# --- CI alias ----------------------------------------------------------------

.PHONY: ci
ci: conformance test-all
	@echo ""
	@echo "==== ci: PASS ===="

# --- Self-lift experiments ---------------------------------------------------
#
# `make self-lift-canonicalizer` runs `provekit-lift` against the
# canonicalizer crate as-is and writes the resulting `.proof` plus a
# human-readable lift-report under `.provekit/self-lifts/canonicalizer/`.
# This is NOT part of the conformance gate; it's a separate experiment
# that surfaces what the auto-lifter can/can't reach on real first-party
# source. Idempotent: re-running with the same source produces the same
# CID (default seed [0x42; 32]). Drift means either the source moved or
# the lifter changed; in either case, inspect lift-report.txt.

PROVEKIT_LIFT := implementations/rust/target/release/provekit-lift
SELF_LIFT_DIR := .provekit/self-lifts/canonicalizer

.PHONY: self-lift-canonicalizer
self-lift-canonicalizer: build-rust
	@echo ">> self-lifting provekit-canonicalizer"
	@mkdir -p $(SELF_LIFT_DIR)
	@rm -f $(SELF_LIFT_DIR)/blake3-512:*.proof
	@out=$$($(PROVEKIT_LIFT) \
		--workspace implementations/rust/provekit-canonicalizer \
		--target-dir $(SELF_LIFT_DIR) --quiet); \
	  echo "  cid: $$out"; \
	  test -f $(SELF_LIFT_DIR)/$$out.proof || \
	    (echo "FAIL: lifter did not write $(SELF_LIFT_DIR)/$$out.proof" && exit 1); \
	  echo "  proof: $(SELF_LIFT_DIR)/$$out.proof"
	@echo "  report: $(SELF_LIFT_DIR)/lift-report.txt"

# --- Cleanup -----------------------------------------------------------------

.PHONY: clean
clean:
	cargo clean --manifest-path implementations/rust/Cargo.toml
	cargo clean --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo clean --manifest-path tools/foundation-keygen/Cargo.toml
	rm -rf implementations/cpp/target
	rm -rf implementations/csharp/Provekit.*/bin implementations/csharp/Provekit.*/obj
	rm -rf node_modules
	cd implementations/go/provekit-self-contracts && rm -f mint-go-self-contracts
	rm -f implementations/*/blake3-512:*.proof
	rm -f blake3-512:*.proof
