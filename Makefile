# ProvekIt — top-level orchestrator
#
# Six-language polyglot. Each language owns its native build tool;
# this Makefile is glue, not a build system. `make ci` runs the same
# gate the GitHub Actions workflow runs (Linux x86_64: Rust/Go/C++/TS/C#/Python).
# Swift is macOS-only; use `make build-swift`, `make test-swift`, `make mint-swift`
# directly on a macOS host — those targets are excluded from the CI aggregates.
#
# Mainline targets:
#   make help        — print this help
#   make ci          — full conformance gate (catalog + protocol + 5 mints + tests)
#   make conformance — catalog + protocol + 5 mint CIDs + self-contract tests
#   make all-mint    — run all 5 mint commands; print CIDs (Linux/CI subset)
#   make test-all    — run every language-native test suite (Linux/CI subset)
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
# Makefile string edit. The dance is identical for every peer kit
# (rust, go, cpp, ts, csharp):
#
#   1. Make your code change in `implementations/<lang>/provekit-self-contracts`
#      (or the language's analog).
#   2. `make mint-<lang>`
#      -> the mint target FAILS and prints the new bundle CID + contractSetCid.
#   3. `cargo run --release --manifest-path tools/foundation-keygen/Cargo.toml \
#         --bin sign-self-contracts -- <lang> <bundle-cid> <contract-set-cid>`
#      -> rewrites `.provekit/self-contracts-attestations/<lang>.json` with
#         a fresh foundation-v0 ed25519 signature over the new CID + contractSetCid.
#   4. `git add .provekit/self-contracts-attestations/<lang>.json && git commit`
#
# The bundle (letter) does not know its own CID. The on-disk attestation
# (envelope) names the CID and is signed externally. See
# `protocol/specs/2026-05-02-bundle-attestation-protocol.md` for the
# generic letter-envelope framing and
# `protocol/specs/2026-05-02-binary-attestation-protocol.md` for the
# binary-specific elaboration. The source tree no longer carries
# machine-local truth about its own bytes for any of the five peer kits.
#
# `CATALOG_CID` is bumped to v1.4.0 here; the constant remains because
# `make help` echoes it. Follow-up: retire it the same way the
# self-contracts CIDs are retired (read from the embedded catalog
# signature attestation).
CATALOG_CID := blake3-512:b0f2030d56c2fddf0ecbd7032bf0344c43e30677930e3b77188fcdc4ca6325d34649e51b2efa97d6985e4be6c43173f803254a7b05fc8bf31b92eb399b60f52f

PROVEKIT := implementations/rust/target/release/provekit
VERIFY_SELF_CONTRACTS := tools/foundation-keygen/target/release/verify-self-contracts
SELF_CONTRACTS_ATTEST_DIR := .provekit/self-contracts-attestations

.PHONY: help
help:
	@echo "ProvekIt — top-level orchestrator"
	@echo ""
	@echo "Mainline:"
	@echo "  make ci             full gate (conformance + test-all) [Linux/CI: 5 peer langs]"
	@echo "  make conformance    catalog + protocol + 5 mint CIDs + self-contract tests"
	@echo "  make all-mint       5 mint commands (Swift excluded: macOS-only, use mint-swift)"
	@echo "  make test-all       language test suites (Swift excluded: macOS-only, use test-swift)"
	@echo ""
	@echo "Per-language build:"
	@echo "  make build-all      build every kit (rust + cpp + go + ts + csharp + java)"
	@echo "  make build-rust     cargo build --release (workspace + tools)"
	@echo "  make build-cpp      clang++ + vendored-blake3"
	@echo "  make build-go       go build per Go module"
	@echo "  make build-ts       pnpm install"
	@echo "  make build-csharp   dotnet build"
	@echo "  make build-java     mvn package + install provekit-lsp-java to ~/.local/bin"
	@echo "  make build-c        cc build of provekit-ir + provekit-lsp-c"
	@echo "  make build-swift    swift build -c release"
	@echo ""
	@echo "Per-language test:"
	@echo "  make test-rust  test-go  test-cpp  test-ts  test-csharp  test-python  test-java  test-c  test-swift"
	@echo ""
	@echo "Per-kit conformance gate (C1-C8 lift-plugin-protocol verifiers):"
	@echo "  make prove-all      all 10 kits (swift excluded: macOS-only)"
	@echo "  make prove-rust  prove-go  prove-cpp  prove-ts  prove-csharp"
	@echo "  make prove-java  prove-python  prove-ruby  prove-zig  prove-c"
	@echo "  make prove-swift    macOS-only"
	@echo ""
	@echo "Self-lift experiments:"
	@echo "  make self-lift-canonicalizer  run provekit-lift against the canonicalizer crate"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean          remove build artifacts"
	@echo ""
	@echo "Pinned CIDs (catalog v1.4.0):"
	@echo "  catalog: $(CATALOG_CID)"
	@echo "  rust:    (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/rust.json"
	@echo "  go:      (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/go.json"
	@echo "  cpp:     (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json"
	@echo "  ts:      (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/ts.json"
	@echo "  csharp:  (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/csharp.json"
	@echo "  swift:   (envelope) $(SELF_CONTRACTS_ATTEST_DIR)/swift.json"

# --- Per-language builds -----------------------------------------------------

# Build every kit's binaries. Useful before `make conformance` or before
# spawning `provekit-linkerd` (which subprocesses kit lifters at lift
# time). Each kit's build target is independent; failures stay isolated.
# NOTE: build-swift is intentionally excluded — it requires a macOS host
# with the Swift toolchain and is not run by Linux CI. Use `make build-swift`
# directly on macOS.
.PHONY: build-all
build-all: build-rust build-cpp build-go build-ts build-csharp build-java

.PHONY: build-rust
build-rust:
	cargo build --release --manifest-path implementations/rust/Cargo.toml
	cargo build --release --manifest-path tools/recompute-spec-cids/Cargo.toml
	cargo build --release --manifest-path tools/foundation-keygen/Cargo.toml

.PHONY: build-cpp
build-cpp:
	tools/build-cpp-self-contracts.sh --build-only
	tools/build-cpp-lsp.sh

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

.PHONY: build-c
build-c:
	$(MAKE) -C implementations/c/provekit-ir all
	$(MAKE) -C implementations/c/provekit-lsp-c all

.PHONY: build-java
build-java:
	mvn package -q -f implementations/java/provekit-lift-java-core/pom.xml
	mkdir -p ~/.local/bin
	cp implementations/java/provekit-lift-java-core/target/appassembler/bin/provekit-lsp-java ~/.local/bin/provekit-lsp-java
	chmod +x ~/.local/bin/provekit-lsp-java

.PHONY: build-swift
build-swift:
	cd implementations/swift && swift build -c release

# --- Mint targets ------------------------------------------------------------

# Each mint target builds its peer + dispatches via `provekit mint --kit=<kit>`.
# The CLI drives the kit's lift-protocol RPC, collects contracts, signs the
# attestation, and writes it to $(SELF_CONTRACTS_ATTEST_DIR)/<lang>.json.
# All 11 kits use the same uniform pipeline; no language-native mint binaries.
#
# For kits whose lifter binary is not yet installed, mint produces an
# empty-set attestation (contractSetCid = BLAKE3-512 of JCS("[]")).
# The attestation is still verified; a missing lifter surfaces as a known gap.

.PHONY: mint-rust
mint-rust: build-rust
	@echo ">> minting rust self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=rust --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/rust.json "$$cset" || \
		(echo "FAIL: rust self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=rust" && exit 1)

.PHONY: mint-go
mint-go: build-rust build-go
	@echo ">> minting go self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=go --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/go.json "$$cset" || \
		(echo "FAIL: go self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=go" && exit 1)

.PHONY: mint-cpp
mint-cpp: build-rust build-cpp
	@echo ">> minting cpp self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=cpp --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json "$$cset" || \
		(echo "FAIL: cpp self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=cpp" && exit 1)

.PHONY: mint-ts
mint-ts: build-rust build-ts
	@echo ">> minting ts self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=ts --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/ts.json "$$cset" || \
		(echo "FAIL: ts self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=ts" && exit 1)

.PHONY: mint-csharp
mint-csharp: build-rust
	@echo ">> minting csharp self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=csharp --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/csharp.json "$$cset" || \
		(echo "FAIL: csharp self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=csharp" && exit 1)

# NOTE: mint-swift requires a macOS host with the Swift toolchain.
# Excluded from all-mint (Linux/CI). Use `make mint-swift` on macOS.
.PHONY: mint-swift
mint-swift: build-rust build-swift
	@echo ">> minting swift self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=swift --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/swift.json "$$cset" || \
		(echo "FAIL: swift self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=swift" && exit 1)

# New kits: lifter binaries not yet available; mint produces empty-set attestation.
# These targets will produce the correct attestation structure; the gap is the
# per-kit lifter, not the substrate pipeline.

.PHONY: mint-java
mint-java: build-rust
	@echo ">> minting java self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=java --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/java.json "$$cset" || \
		(echo "FAIL: java self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=java" && exit 1)

.PHONY: mint-python
mint-python: build-rust
	@echo ">> minting python self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=python --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/python.json "$$cset" || \
		(echo "FAIL: python self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=python" && exit 1)

.PHONY: mint-ruby
mint-ruby: build-rust
	@echo ">> minting ruby self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=ruby --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/ruby.json "$$cset" || \
		(echo "FAIL: ruby self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=ruby" && exit 1)

.PHONY: mint-zig
mint-zig: build-rust
	@echo ">> minting zig self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=zig --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/zig.json "$$cset" || \
		(echo "FAIL: zig self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=zig" && exit 1)

.PHONY: mint-c
mint-c: build-rust
	@echo ">> minting c self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=c --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/c.json "$$cset" || \
		(echo "FAIL: c self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=c" && exit 1)

# NOTE: mint-swift excluded from all-mint (macOS-only). New kits (java/python/
# ruby/zig/c) produce empty-set attestations until their lifters are wired up.
.PHONY: all-mint
all-mint: mint-rust mint-go mint-cpp mint-ts mint-csharp
	@echo ""
	@echo "==== all 5 core self-contract CIDs match pinned values ===="
	@printf "  %-8s  %s\n" "rust"   "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/rust.json)"
	@printf "  %-8s  %s\n" "go"     "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/go.json)"
	@printf "  %-8s  %s\n" "cpp"    "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json)"
	@printf "  %-8s  %s\n" "ts"     "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/ts.json)"
	@printf "  %-8s  %s\n" "csharp" "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/csharp.json)"

# --- Per-kit prove (C1-C8 conformance gate) ----------------------------------
#
# Each `prove-<kit>` target:
#   1. Builds the kit's lifter binary.
#   2. Runs `provekit prove --kit=<kit>`, which:
#      - Spawns the kit's lifter via JSON-RPC.
#      - Drives the initialize -> lift -> shutdown sequence.
#      - Runs C1-C8 verifiers against the captured RPC messages.
#   3. Exits 0 iff all 8 contracts hold.
#
# NOTE: prove-swift requires a macOS host (Swift toolchain). It is excluded
# from prove-all but can be run directly on macOS.
#
# Kits with no lifter yet (java/python/ruby/zig/c) exit 2 (user error) until
# their lifters are wired up. They are listed in prove-all so CI knows which
# need follow-up.

.PHONY: prove-rust
prove-rust: build-rust
	@echo ">> proving rust lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=rust

.PHONY: prove-go
prove-go: build-rust build-go
	@echo ">> proving go lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=go

.PHONY: prove-cpp
prove-cpp: build-rust build-cpp
	@echo ">> proving cpp lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=cpp

.PHONY: prove-ts
prove-ts: build-rust build-ts
	@echo ">> proving ts lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=ts

.PHONY: prove-csharp
prove-csharp: build-rust build-csharp
	@echo ">> proving csharp lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=csharp

# macOS-only: requires Swift toolchain.
.PHONY: prove-swift
prove-swift: build-rust build-swift
	@echo ">> proving swift lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=swift

.PHONY: prove-java
prove-java: build-rust build-java
	@echo ">> proving java lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=java

.PHONY: prove-python
prove-python: build-rust
	@echo ">> proving python lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=python

.PHONY: prove-ruby
prove-ruby: build-rust
	@echo ">> proving ruby lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=ruby

.PHONY: prove-zig
prove-zig: build-rust build-zig
	@echo ">> proving zig lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=zig

.PHONY: prove-c
prove-c: build-rust build-c
	@echo ">> proving c lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=c

# prove-all: run C1-C8 gate for the Linux/CI subset (swift excluded: macOS-only).
# Kits without a wired lifter exit 2 (user error); all 10 targets are listed
# so CI reports which need follow-up. prove-swift runs separately on macos-latest.
.PHONY: prove-all
prove-all: prove-rust prove-go prove-cpp prove-ts prove-csharp prove-java prove-python prove-ruby prove-zig prove-c
	@echo ""
	@echo "==== prove-all: complete ===="

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
conformance: catalog-verify protocol-verify all-mint test-self-contracts
	@echo ""
	@echo "==== conformance: PASS ===="

# --- Self-contracts contract-assertion tests --------------------------------
#
# `all-mint` proves each peer kit's bundle round-trips to its pinned CID.
# That catches CID drift, but it does NOT catch a contract-assertion test
# being weakened or deleted (e.g. an R1..R15 rule from
# `protocol/specs/2026-04-30-protocol-catalog-format.md` losing its check).
# `test-self-contracts` runs the kit-native unit tests that encode those
# rule assertions, so the conformance gate fails when a regression flips
# any one of them.
#
# Today only the Rust kit ships catalog-format contract-assertion tests
# (`implementations/rust/provekit-self-contracts/src/catalog_format.rs`,
# 19 `#[test]` fns covering R1..R15). The go/cpp/ts/csharp self-contracts
# packages currently only carry the mint binary; once they grow their own
# catalog-format test suites, add `test-self-contracts-<lang>` targets
# alongside the rust one and append them to the aggregate dep list below.

.PHONY: test-self-contracts
test-self-contracts: test-self-contracts-rust

.PHONY: test-self-contracts-rust
test-self-contracts-rust:
	cargo test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-self-contracts --lib

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
	@echo "test-cpp: LSP lifecycle integration test"
	sh implementations/cpp/provekit-lsp-cpp/test_lsp.sh implementations/cpp/target/provekit-lsp-cpp
	@echo "test-cpp: mint round-trip also covered by mint-cpp"

.PHONY: test-ts
test-ts:
	pnpm test

.PHONY: test-csharp
test-csharp: build-csharp
	dotnet test implementations/csharp/Provekit.sln --nologo --verbosity quiet

.PHONY: test-c
test-c: build-c
	$(MAKE) -C implementations/c/provekit-ir test
	$(MAKE) -C implementations/c/provekit-lsp-c test

.PHONY: test-python
test-python:
	cd implementations/python/provekit-lift-py-tests && \
		pip install --quiet -e . && \
		pip install --quiet pytest && \
		pytest

.PHONY: test-java
test-java: build-java
	mvn test -q -f implementations/java/provekit-lift-java-core/pom.xml

.PHONY: test-swift
test-swift: build-swift
	cd implementations/swift && swift run conformance
	cd implementations/swift && swift run test-swift-lsp

.PHONY: test-zig
test-zig:
	cd implementations/zig/provekit-ir && zig build test
	cd implementations/zig/provekit-lift-zig && zig build test
	cd implementations/zig/provekit-lsp-zig && zig build test
	cd implementations/zig/provekit-lsp-zig && zig build
	@echo "test-zig: LSP lifecycle integration test"
	sh implementations/zig/provekit-lsp-zig/test_lsp.sh

.PHONY: build-zig
build-zig:
	cd implementations/zig/provekit-ir && zig build
	cd implementations/zig/provekit-lsp-zig && zig build

# NOTE: test-swift is intentionally excluded from test-all — it requires a
# macOS host with the Swift toolchain. Use `make test-swift` on macOS.
.PHONY: test-all
test-all: test-rust test-go test-ts test-csharp test-python test-java
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
