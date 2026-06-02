# ProvekIt: top-level orchestrator
#
# Twelve-kit polyglot. TypeScript is the center surface, but every kit
# owns its native build tool;
# this Makefile is glue, not a build system. `make ci` runs the Linux-profile
# gate used by the main GitHub Actions job.
# Swift is macOS-only; use `make build-swift`, `make test-swift`, `make mint-swift`
# directly on a macOS host: those targets are excluded from the CI aggregates.
#
# Mainline targets:
#   make help: print this help
#   make ci: Linux-profile gate (catalog + protocol + live mints + tests)
#   make conformance: catalog + protocol + live mint CIDs + self-contract tests
#   make cross-language-proof-parity: Java/Go/Python/Rust emit + materialize + recognize + mint + prove + contradiction gate
#   make cross-language-proof-parity-extra: opt-in TypeScript/Zig/Scala/Swift parity lanes
#   make all-mint: run all 11 Linux-profile mint commands; print CIDs
#   make bootstrap-self-contracts: re-sign attestations from live artifacts
#   make test-all: run the Linux native test aggregate
#
# Per-language targets:
#   make build-rust: cargo build --release for workspace + tools
#   make build-cpp: vendored-blake3 clang++ build of the C++ orchestrator
#   make test-rust / test-go / test-ts / test-csharp / test-python / test-ruby / test-php / test-scala / test-c
#
# Determinism:
#   make ci is the local Linux-profile contract. If it's green, the non-Swift
#   self-contracts round-trip to their pinned CIDs, the v1.6.2 catalog hash
#   matches, and the Linux native test aggregate passes. The GitHub workflow
#   adds macOS Swift and per-kit verifier jobs.

.DEFAULT_GOAL := help

# --- Pinned CIDs ------------------------------------------------------------
#
# Bumping a self-contracts CID is now an explicit attestation event, NOT a
# Makefile string edit. The dance is identical for every peer kit
# (rust, go, cpp, ts, csharp):
#
#   1. Make your code change in `implementations/<lang>/provekit-self-contracts`
#      (or the language's analog).
#   2. `make bootstrap-self-contracts`
#      -> builds the selected kit toolchains, mints verifier-loadable proof
#         artifacts, and re-signs `.provekit/self-contracts-attestations/*.json`
#         from the live bundle CID + contractSetCid.
#   3. `git add .provekit/self-contracts-attestations/<lang>.json && git commit`
#
# The bundle (letter) does not know its own CID. The on-disk attestation
# (envelope) names the CID and is signed externally. See
# `protocol/specs/2026-05-02-bundle-attestation-protocol.md` for the
# generic letter-envelope framing and
# `protocol/specs/2026-05-02-binary-attestation-protocol.md` for the
# binary-specific elaboration. The source tree no longer carries
# machine-local truth about its own bytes for any of the five peer kits.
#
# `CATALOG_CID` is bumped to v1.6.2 here; the constant remains because
# `make help` echoes it. Follow-up: retire it the same way the
# self-contracts CIDs are retired (read from the embedded catalog
# signature attestation).
CATALOG_CID := blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f

PROVEKIT := implementations/rust/target/release/provekit
VERIFY_SELF_CONTRACTS := tools/foundation-keygen/target/release/verify-self-contracts
SELF_CONTRACTS_ATTEST_DIR := .provekit/self-contracts-attestations
CONFORMANCE_PROFILE ?= linux
CONFORMANCE_JOBS ?= 4
RUBY ?= $(shell for p in /usr/local/opt/ruby/bin/ruby /opt/homebrew/opt/ruby/bin/ruby /usr/local/bin/ruby /opt/homebrew/bin/ruby; do if [ -x "$$p" ]; then echo "$$p"; exit; fi; done; command -v ruby || echo ruby)
PYTHON ?= $(shell command -v python3 || echo python3)
PIP ?= pip3 --python $(PYTHON)
MVN ?= mvn
LOCAL_BIN ?= /tmp/provekit-local-bin
SCALA_CLI ?= scala-cli
PARITY_PYTHON_VENV ?= /tmp/provekit-cross-language-parity-python
PARITY_PYTHON_BIN := $(PARITY_PYTHON_VENV)/bin
PARITY_PYTHON := $(PARITY_PYTHON_BIN)/python
BCARGO_PYTHON_VENV ?= /tmp/provekit-bcargo-python-kit-env
BCARGO_PYTHON_BIN := $(BCARGO_PYTHON_VENV)/bin
BCARGO_PYTHON := $(BCARGO_PYTHON_BIN)/python
BCARGO_PYTHON_ENV_STAMP := $(BCARGO_PYTHON_VENV)/.provekit-python-kits.stamp
PYTHON_KIT_EDITABLES = \
	-e examples/provekit-shim-python-sqlite3 \
	-e examples/provekit-shim-python-aiosqlite \
	-e examples/provekit-shim-python-requests \
	-e implementations/python/libprovekit-py \
	-e implementations/python/provekit-lift-py-tests \
	-e implementations/python/provekit-lift-python-source \
	-e implementations/python/provekit-emit-python-pytest \
	-e implementations/python/provekit-emit-python-unittest \
	-e implementations/python/provekit-emit-python-hypothesis \
	-e implementations/python/provekit-realize-python-core \
	-e implementations/python/provekit-realize-python-sqlite3 \
	-e implementations/python/provekit-realize-python-aiosqlite \
	-e implementations/python/provekit-realize-python-requests
BCARGO ?= $(CURDIR)/bin/bcargo
CARGO_LOCAL ?= cargo
ifeq ($(CI),)
ifeq ($(USE_BCARGO),0)
CARGO ?= $(CARGO_LOCAL)
else
CARGO ?= $(BCARGO)
endif
else
CARGO ?= $(CARGO_LOCAL)
endif
BCARGO_ACTIVE := $(filter bcargo,$(notdir $(firstword $(CARGO))))
CARGO_SYNC_BINS = $(if $(BCARGO_ACTIVE),$(CARGO) $(foreach bin,$(1),--sync-bin $(bin)),$(CARGO))
JAVA_HOME ?= $(shell for d in /usr/local/opt/openjdk /opt/homebrew/opt/openjdk; do if [ -x "$$d/bin/java" ]; then echo "$$d"; exit; fi; done)
export JAVA_HOME
ifeq ($(strip $(JAVA_HOME)),)
export PATH := $(LOCAL_BIN):$(dir $(RUBY)):$(PATH)
else
export PATH := $(LOCAL_BIN):$(dir $(RUBY)):$(JAVA_HOME)/bin:$(PATH)
endif

.PHONY: help
help:
	@echo "ProvekIt: top-level orchestrator"
	@echo ""
	@echo "Mainline:"
	@echo "  make ci             Linux-profile gate (conformance + test-all)"
	@echo "  make conformance    catalog + protocol + 11 mint CIDs + self-contract tests"
	@echo "  make cross-language-proof-parity"
	@echo "                       Java/Go/Python/Rust emit + materialize + recognize + mint + prove + contradiction gate"
	@echo "  make cross-language-proof-parity-extra"
	@echo "                       opt-in TypeScript/Zig/Scala/Swift parity lanes"
	@echo "  make all-mint       11 mint commands (Swift excluded: macOS-only, use mint-swift)"
	@echo "  make bug-zoo        replay executable bug specimens through source-routed CLI"
	@echo "  make bootstrap-self-contracts"
	@echo "                       re-sign attestations from live kit artifacts"
	@echo "                       override: CONFORMANCE_PROFILE=all CONFORMANCE_JOBS=8"
	@echo "  make test-all       language test suites (Swift excluded: macOS-only, use test-swift)"
	@echo ""
	@echo "Per-language build:"
	@echo "  make build-all      build every kit (rust + cpp + go + ts + csharp + java + python + ruby + scala)"
	@echo "  make build-rust     cargo build --release (workspace + tools)"
	@echo "  make build-cpp      clang++ + vendored-blake3"
	@echo "  make build-go       go build per Go module"
	@echo "  make build-ts       pnpm install"
	@echo "  make build-python   pip-install Python realize kits and shim packages"
	@echo "  make build-csharp   dotnet build"
	@echo "  make build-java     mvn package + install provekit-lsp-java to ~/.local/bin"
	@echo "  make build-scala    scala-cli compile Scala emit kits"
	@echo "  make build-c        cc build of C IR, lifters, LSP, and self-contracts"
	@echo "  make build-swift    swift build -c release"
	@echo ""
	@echo "Per-language test:"
	@echo "  make test-rust  test-go  test-cpp  test-ts  test-csharp  test-python  test-ruby  test-php  test-java  test-scala  test-c  test-swift"
	@echo ""
	@echo "Per-kit conformance gate (C1-C8 lift-plugin-protocol verifiers):"
	@echo "  make prove-all      all 12 Linux kits (swift excluded: macOS-only)"
	@echo "  make prove-rust  prove-go  prove-cpp  prove-ts  prove-csharp  prove-clr-bytecode"
	@echo "  make prove-java  prove-python  prove-ruby  prove-zig  prove-c"
	@echo "  make prove-swift    macOS-only"
	@echo ""
	@echo "Self-lift experiments:"
	@echo "  make self-lift-canonicalizer  run provekit-lift against the canonicalizer crate"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean          remove build artifacts"
	@echo "  make mint-typescript-language-signature"
	@echo "                       mint the draft TypeScript source language-signature catalog"
	@echo ""
	@echo "Pinned CIDs (catalog v1.6.2):"
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
# NOTE: build-swift is intentionally excluded: it requires a macOS host
# with the Swift toolchain and is not run by Linux CI. Use `make build-swift`
# directly on macOS.
.PHONY: build-all
build-all: build-rust build-cpp build-go build-ts build-csharp build-java build-python build-ruby build-scala

.PHONY: build-rust
build-rust:
	$(call CARGO_SYNC_BINS,provekit provekit-lift) build --release --manifest-path implementations/rust/Cargo.toml
	$(CARGO) build --release --manifest-path tools/recompute-spec-cids/Cargo.toml
	$(call CARGO_SYNC_BINS,verify-self-contracts sign-self-contracts) build --release --manifest-path tools/foundation-keygen/Cargo.toml

.PHONY: build-rust-cli
build-rust-cli:
	$(call CARGO_SYNC_BINS,provekit) build --release --manifest-path implementations/rust/Cargo.toml -p provekit-cli

.PHONY: build-rust-self-contract-verifier
build-rust-self-contract-verifier:
	$(call CARGO_SYNC_BINS,verify-self-contracts) build --release --manifest-path tools/foundation-keygen/Cargo.toml --bin verify-self-contracts

.PHONY: build-rust-mint-tools
build-rust-mint-tools: build-rust-cli build-rust-self-contract-verifier

.PHONY: check-macos-swift-rust-scope
check-macos-swift-rust-scope:
	@mint_dry="$$( $(MAKE) --no-print-directory -n mint-swift )"; \
	prove_dry="$$( $(MAKE) --no-print-directory -n prove-swift )"; \
	cli_tree="$$($(CARGO_LOCAL) tree --manifest-path implementations/rust/Cargo.toml -p provekit-cli --edges normal,build --depth 4)"; \
	cli_cmd='$(call CARGO_SYNC_BINS,provekit) build --release --manifest-path implementations/rust/Cargo.toml -p provekit-cli'; \
	verifier_cmd='$(call CARGO_SYNC_BINS,verify-self-contracts) build --release --manifest-path tools/foundation-keygen/Cargo.toml --bin verify-self-contracts'; \
	full_workspace_cmd='$(call CARGO_SYNC_BINS,provekit provekit-lift) build --release --manifest-path implementations/rust/Cargo.toml'; \
	echo "$$mint_dry" | grep -F -- "$$cli_cmd" >/dev/null || \
		(echo "FAIL: mint-swift must build only the provekit CLI, not the full Rust workspace" && exit 1); \
	echo "$$mint_dry" | grep -F -- "$$verifier_cmd" >/dev/null || \
		(echo "FAIL: mint-swift must build the self-contract verifier it invokes" && exit 1); \
	echo "$$prove_dry" | grep -F -- "$$cli_cmd" >/dev/null || \
		(echo "FAIL: prove-swift must build only the provekit CLI, not the full Rust workspace" && exit 1); \
	if echo "$$mint_dry" | grep -F -x -- "$$full_workspace_cmd" >/dev/null; then \
		echo "FAIL: mint-swift still pulls the full Rust workspace"; exit 1; \
	fi; \
	if echo "$$prove_dry" | grep -F -x -- "$$full_workspace_cmd" >/dev/null; then \
		echo "FAIL: prove-swift still pulls the full Rust workspace"; exit 1; \
	fi; \
	if echo "$$mint_dry" | grep -F -- 'tools/recompute-spec-cids/Cargo.toml' >/dev/null; then \
		echo "FAIL: mint-swift must not build recompute-spec-cids"; exit 1; \
	fi; \
	if echo "$$cli_tree" | grep -F -- 'provekit-realize-rust-core' >/dev/null; then \
		echo "FAIL: provekit-cli must not pull the Rust realize kit into macOS Swift mint builds"; exit 1; \
	fi; \
	if echo "$$cli_tree" | grep -F -- '/examples/provekit-shim-' >/dev/null; then \
		echo "FAIL: provekit-cli must not pull Rust shim example crates into macOS Swift mint builds"; exit 1; \
	fi; \
	if echo "$$cli_tree" | grep -E 'smoke-test-e2e|provekit-bridgeworks|provekit-supply-chain-rails|provekit-protocol-switchyard|bug-zoo' >/dev/null; then \
		echo "FAIL: provekit-cli must not pull menagerie crates into macOS Swift mint builds"; exit 1; \
	fi

.PHONY: build-cpp
build-cpp:
	tools/build-cpp-self-contracts.sh --build-only
	tools/build-cpp-lift.sh
	tools/build-cpp-source-lift.sh
	tools/build-cpp-lsp.sh

.PHONY: build-go
build-go:
	cd implementations/go && go build ./...
	cd implementations/go/provekit-ir-symbolic && go build ./...
	cd implementations/go/provekit-self-contracts && go build ./...
	cd implementations/go/provekit-lift-go-tests && go build ./...
	cd implementations/go/provekit-lift-go && go build ./...

.PHONY: build-ts
build-ts:
	pnpm install --frozen-lockfile
	# Each TS realize kit resolves its shim .proof from its OWN node_modules
	# (file: dep on the example shim) and needs @ipld/dag-cbor + @noble/hashes
	# for the CBOR decode. The root install does not provision these per-kit
	# deps (the kits are npm package-lock.json based, outside the pnpm root),
	# so install each kit explicitly. Without this the kit RPC returns
	# SHIM_NOT_FOUND and SQL/migrate materialize tests refuse.
	npm --prefix implementations/typescript/provekit-emit-typescript-vitest ci
	npm --prefix implementations/typescript/provekit-realize-typescript-core ci
	npm --prefix implementations/typescript/provekit-realize-typescript-better-sqlite3 ci
	npm --prefix implementations/typescript/provekit-realize-typescript-pg ci

.PHONY: build-csharp
build-csharp:
	dotnet build implementations/csharp/Provekit.sln --configuration Release --nologo

.PHONY: build-c
build-c:
	$(MAKE) -C implementations/c/provekit-ir all
	$(MAKE) -C implementations/c/provekit-lift all
	$(MAKE) -C implementations/c/provekit-lift-core all
	$(MAKE) -C implementations/c/provekit-lift-c-sparse all
	$(MAKE) -C implementations/c/provekit-lift-c-kernel-doc all
	$(MAKE) -C implementations/c/provekit-lift-c-assertions all
	$(MAKE) -C implementations/c/provekit-realize-c-core all
	$(MAKE) -C implementations/c/provekit-lsp-c all
	$(MAKE) -C implementations/c/provekit-self-contracts lib

.PHONY: build-c-self-contracts
build-c-self-contracts:
	# Build the c self-contracts orchestrator binary. Depends on the c
	# Side B static library (libprovekit-self-contracts.a); the orchestrator
	# Makefile invokes the Side B build as a sub-make if needed.
	$(MAKE) -C implementations/c/mint-c-self-contracts

.PHONY: build-java
build-java: build-java-self-contracts
	# provekit-lift-java-core depends on the sibling provekit-ir module.
	# Use the parent pom + `-pl ... -am` (also-make) so dependencies are
	# built first.
	$(MVN) package -q -f implementations/java/pom.xml -pl provekit-lift-java-core -am
	# provekit-realize-java-core ships the shaded `provekit-realize-java.jar`
	# that libprovekit's platform_semantics_loader spawns over JSON-RPC for
	# every `target=java` carrier registration. Without packaging it here,
	# rust integration tests that touch the java carrier (e.g.
	# `lower_java_carrier_registration_points_at_required_fixture_set`) fail
	# with `Unable to access jarfile provekit-realize-java.jar`.
	$(MVN) package -q -f implementations/java/pom.xml -pl provekit-realize-java-core -am -DskipTests
	mkdir -p $(LOCAL_BIN)
	cp implementations/java/provekit-lift-java-core/target/appassembler/bin/provekit-lsp-java $(LOCAL_BIN)/provekit-lsp-java
	chmod +x $(LOCAL_BIN)/provekit-lsp-java

.PHONY: build-python
build-python:
	$(PIP) install --quiet --no-cache-dir \
		-e examples/provekit-shim-python-sqlite3 \
		-e examples/provekit-shim-python-aiosqlite \
		-e examples/provekit-shim-python-requests \
		-e implementations/python/provekit-realize-python-core \
		-e implementations/python/provekit-realize-python-sqlite3 \
		-e implementations/python/provekit-realize-python-aiosqlite \
		-e implementations/python/provekit-realize-python-requests

.PHONY: build-scala
build-scala:
	$(SCALA_CLI) compile implementations/scala/provekit-emit-scala-scalatest --server=false --scalac-option -deprecation
	$(SCALA_CLI) compile implementations/scala/provekit-lift-scala-source --server=false --scalac-option -deprecation

.PHONY: build-java-self-contracts
build-java-self-contracts:
	# Build the self-contracts orchestrator's shaded jar (BouncyCastle +
	# provekit-claim-envelope bundled). The jar lands at
	# implementations/java/provekit-java-self-contracts/target/provekit-java-self-contracts.jar
	# and the lift manifest spawns it with `java -jar`.
	$(MVN) -q -f implementations/java/pom.xml -pl provekit-java-self-contracts -am package -DskipTests

.PHONY: build-ruby
build-ruby:
	cd implementations/ruby/ext/provekit_blake3 && $(RUBY) extconf.rb && $(MAKE)
	cd implementations/ruby && $(RUBY) -S bundle exec $(RUBY) -Ilib -e 'require "provekit"; abort unless Provekit::Blake3.hex("provekit").start_with?("blake3-512:")'

.PHONY: build-swift
# Debug, not release. ~90% of this build is the swift-syntax dependency (the
# Swift parser the source lifter needs), and compiling it with full release
# optimization is most of the multi-minute tax. Debug is safe here: minted
# CIDs are content-addressed (JCS+blake3 over contract data), so binary
# optimization level cannot change them, and CI fixtures are tiny so lifter
# runtime perf is irrelevant. Override with `SWIFT_BUILD_CONFIG=release` if a
# release artifact is ever needed.
SWIFT_BUILD_CONFIG ?= debug
build-swift:
	cd implementations/swift && swift build -c $(SWIFT_BUILD_CONFIG)

# --- Mint targets ------------------------------------------------------------

# Each mint target builds its peer + dispatches via a `--kit=<alias>` entry
# declared in `.provekit/config.toml`. The CLI does not carry a built-in kit
# list; aliases resolve to project roots and lift manifests from config.
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
	@mint_out=$$($(PROVEKIT) mint --project implementations/rust --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/rust.json "$$cset" || \
		(echo "FAIL: rust self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --project implementations/rust" && exit 1)

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
mint-swift: build-rust-mint-tools build-swift
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
mint-java: build-rust build-java-self-contracts
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
mint-ruby: build-rust build-ruby
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
mint-zig: build-rust build-zig
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
mint-c: build-rust build-c-self-contracts
	@echo ">> minting c self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=c --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/c.json "$$cset" || \
		(echo "FAIL: c self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=c" && exit 1)

.PHONY: mint-php
mint-php: build-rust
	@echo ">> minting php self-contracts"
	@mint_out=$$($(PROVEKIT) mint --kit=php --quiet); \
	cid=$$(echo "$$mint_out" | head -1); \
	cset=$$(echo "$$mint_out" | grep '^contractSetCid:' | sed 's/^contractSetCid: //'); \
	echo "  cid:            $$cid"; \
	echo "  contractSetCid: $$cset"; \
	$(VERIFY_SELF_CONTRACTS) $(SELF_CONTRACTS_ATTEST_DIR)/php.json "$$cset" || \
		(echo "FAIL: php self-contracts attestation rejected; re-mint and commit:" && \
		 echo "      $(PROVEKIT) mint --kit=php" && exit 1)

# NOTE: all-mint runs 11 of 12 kits (Linux/CI subset).
# Excluded: swift (macOS-only; use mint-swift on macOS).
# zig and c were added after their Side A merges (#283, #272) and are included.
# php was added after its self-contracts attestation was signed (#393).
.PHONY: all-mint
all-mint: mint-rust mint-go mint-cpp mint-ts mint-csharp mint-java mint-python mint-ruby mint-c mint-zig mint-php
	@echo ""
	@echo "==== all 11 core self-contract CIDs match pinned values ===="
	@printf "  %-8s  %s\n" "rust"   "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/rust.json)"
	@printf "  %-8s  %s\n" "go"     "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/go.json)"
	@printf "  %-8s  %s\n" "cpp"    "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/cpp.json)"
	@printf "  %-8s  %s\n" "ts"     "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/ts.json)"
	@printf "  %-8s  %s\n" "csharp" "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/csharp.json)"
	@printf "  %-8s  %s\n" "java"   "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/java.json)"
	@printf "  %-8s  %s\n" "python" "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/python.json)"
	@printf "  %-8s  %s\n" "ruby"   "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/ruby.json)"
	@printf "  %-8s  %s\n" "c"      "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/c.json)"
	@printf "  %-8s  %s\n" "zig"    "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/zig.json)"
	@printf "  %-8s  %s\n" "php"    "(envelope: $(SELF_CONTRACTS_ATTEST_DIR)/php.json)"

# --- Per-kit prove (C1-C8 conformance gate) ----------------------------------
#
# Each `prove-<kit>` target:
#   1. Builds the kit's lifter binary.
#   2. Runs `provekit prove --kit=<alias>`, which resolves the alias from
#      `.provekit/config.toml` and:
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

.PHONY: prove-clr-bytecode
prove-clr-bytecode: build-rust build-csharp
	@echo ">> proving clr-bytecode lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=clr-bytecode

# macOS-only: requires Swift toolchain.
.PHONY: prove-swift
prove-swift: build-rust-cli build-swift
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
prove-ruby: build-rust build-ruby
	@echo ">> proving ruby lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=ruby

.PHONY: prove-zig
prove-zig: build-rust build-zig
	@echo ">> proving zig lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=zig

.PHONY: prove-c
prove-c: build-rust build-c build-c-self-contracts
	@echo ">> proving c lift-plugin-protocol conformance (C1-C8)"
	$(PROVEKIT) prove --kit=c

# prove-all: run C1-C8 gate for the Linux/CI subset (swift excluded: macOS-only).
# Kits without a wired lifter exit 2 (user error); all 12 targets are listed
# so CI reports which need follow-up. prove-swift runs separately on macos-latest.
.PHONY: prove-all
prove-all: prove-rust prove-go prove-cpp prove-ts prove-csharp prove-clr-bytecode prove-java prove-python prove-ruby prove-zig prove-c
	@echo ""
	@echo "==== prove-all: complete ===="

# --- Conformance gate --------------------------------------------------------

# Default invocation (no args) is read-only since audit #180 fix; --verify
# is now a no-op alias retained because protocol-catalog-format.md §5
# names it literally. Either form is safe; only --write mutates the catalog.
.PHONY: catalog-verify
catalog-verify:
	$(CARGO) run --release --manifest-path tools/recompute-spec-cids/Cargo.toml -- --verify

.PHONY: c11-cursorkind-check
c11-cursorkind-check:
	python3 tools/generate-c11-from-cursorkind.py --check

.PHONY: mint-typescript-language-signature
mint-typescript-language-signature:
	menagerie/typescript-language-signature/mint.sh

.PHONY: protocol-verify
protocol-verify: build-rust
	$(PROVEKIT) verify-protocol --signed

.PHONY: cid-stability-check
cid-stability-check:
	@echo "=== ProofIR resolved round-trip CID stability ==="
	python3 bootstrap/scripts/cid_stability_check.py

.PHONY: conformance
conformance: c11-cursorkind-check catalog-verify protocol-verify all-mint test-mint-kit-integration-pins test-self-contracts conformance-region-fixture cross-kit-conformance cid-stability-check
	@echo ""
	@echo "==== conformance: PASS ===="

.PHONY: test-mint-kit-integration-pins
test-mint-kit-integration-pins: all-mint
	@echo "=== mint kit integration pins: rust/cpp CID gates ==="
	CI=1 $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test mint_kit_integration \
		kits_with_real_contracts_produce_nonempty_contract_set
	CI=1 $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test mint_kit_integration \
		rust_kit_contract_set_cid_is_pinned_to_self_contracts_canonical
	CI=1 $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test mint_kit_integration \
		cpp_kit_contract_set_cid_is_pinned_to_self_contracts_canonical

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
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-self-contracts --lib

# --- Cross-kit conformance fixtures ------------------------------------------
#
# Byte-pinned fixtures that every kit must produce the same CID for.
# Currently only the Rust kit has Sort::Region support; other kits
# gracefully skip until their per-kit regen lands.

.PHONY: conformance-region-fixture
conformance-region-fixture:
	@echo "=== Region+Dependent byte-pinned fixture ==="
	@$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-canonicalizer --test conformance_region_dependent

.PHONY: cross-kit-conformance
cross-kit-conformance:
	@echo "=== Catalog-pinned cross-kit conformance fixtures ==="
	$(CARGO) run --release --manifest-path tools/cross-kit-conformance/Cargo.toml -- \
		--profile $(CONFORMANCE_PROFILE) --jobs $(CONFORMANCE_JOBS)

.PHONY: cross-language-proof-parity-python-env
cross-language-proof-parity-python-env:
	$(PYTHON) -m venv $(PARITY_PYTHON_VENV)
	$(PARITY_PYTHON) -m pip install --quiet --upgrade pip
	$(PARITY_PYTHON) -m pip install --quiet \
		pytest \
		-e implementations/python/provekit-lift-py-tests \
		-e examples/provekit-shim-python-requests \
		-e implementations/python/provekit-emit-python-pytest \
		-e implementations/python/provekit-lift-python-source \
		-e implementations/python/provekit-realize-python-core \
		-e implementations/python/provekit-realize-python-requests

.PHONY: bcargo-python-kit-env
bcargo-python-kit-env: $(BCARGO_PYTHON_ENV_STAMP)

$(BCARGO_PYTHON_ENV_STAMP): Makefile $(wildcard implementations/python/*/pyproject.toml examples/provekit-shim-python-*/pyproject.toml)
	$(PYTHON) -m venv $(BCARGO_PYTHON_VENV)
	$(BCARGO_PYTHON) -m pip install --quiet --upgrade pip
	$(BCARGO_PYTHON) -m pip install --quiet --no-cache-dir pytest $(PYTHON_KIT_EDITABLES)
	mkdir -p $(dir $(BCARGO_PYTHON_ENV_STAMP))
	touch $(BCARGO_PYTHON_ENV_STAMP)

.PHONY: check-cross-language-proof-parity-scope
check-cross-language-proof-parity-scope:
	tools/check-cross-language-proof-parity-scope.sh

.PHONY: check-cargo-entrypoint
check-cargo-entrypoint:
	tools/check-cargo-entrypoint.sh

.PHONY: cross-language-proof-parity
cross-language-proof-parity: build-java cross-language-proof-parity-python-env
	@echo "=== Cross-language proof parity: emit/materialize/recognize/mint/prove/contradiction lanes for Java, Go, Python, Rust ==="
	$(CARGO) build --manifest-path implementations/rust/Cargo.toml \
		-p provekit-realize-rust-core --bin provekit-realize-rust
	@echo "--- emit parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_java_junit \
		emit_java_junit_uses_checked_in_java_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_java_testng \
		emit_java_testng_uses_checked_in_java_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_go_testing \
		emit_go_testing_uses_checked_in_go_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_go_testify \
		emit_go_testify_dispatches_separate_emitter_and_compile_checks
	PYTHON=$(PARITY_PYTHON) PATH=$(PARITY_PYTHON_BIN):$(PATH) $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_python_pytest \
		emit_python_pytest_uses_checked_in_python_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_rust_cargo_test \
		emit_rust_cargo_test_uses_checked_in_rust_double_registration
	@echo "--- materialize parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_materialize_proof_load \
		materialize_json_client_jackson_loads_from_proof_and_compiles
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test go_realize_materialize \
		go_materialize_uses_checked_in_go_double_realize_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test go_realize_materialize \
		go_materialize_uses_body_template_from_go_module_proof
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test go_realize_materialize \
		go_dependency_proofs_are_resolved_by_configured_go_kit
	PATH=$(PARITY_PYTHON_BIN):$(PATH) $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_materialize_integration \
		materialize_python_uses_checked_in_python_double_realize_registration
	PATH=$(PARITY_PYTHON_BIN):$(PATH) $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_materialize_integration \
		materialize_python_requests_example_uses_python_library_shim
	$(PARITY_PYTHON) -m pytest \
		implementations/python/provekit-realize-python-requests/tests/test_rpc.py \
		-q -k test_resolve_dependency_proofs_returns_distribution_proof_bytes
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_materialize_integration \
		materialize_out_dir_writes_materialized_copy_and_leaves_source_unchanged
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_materialize_integration \
		materialize_rust_reqwest_uses_checked_in_rust_double_realize_registration
	@echo "--- recognizer parity ---"
	$(MVN) -q -f implementations/java/pom.xml \
		-pl provekit-lift-java-source -am \
		-Dtest=RecognizeHandlerTest,JavaSugarBindingLifterTest test
	(cd implementations/go/provekit-lift-go && \
		go test ./... -run 'Test(SugarBody|Recognize)')
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_recognize_go_parity \
		go_recognize_write_self_resolves_project_proofs_and_proves -- --nocapture
	$(PARITY_PYTHON) -m pytest \
		implementations/python/provekit-lift-python-source/tests/test_bind_lifter.py \
		-q -k 'sugar_body or recognize'
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-walk --bin provekit-walk-rpc \
		sugar_body -- --nocapture
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-walk --bin provekit-walk-rpc \
		recognize -- --nocapture
	@echo "--- mint parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_java_production_bridge \
		java_mint_uses_checked_in_java_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_go_production_bridge \
		go_mint_auto_writes_body_discharge_bridge
	PYTHON=$(PARITY_PYTHON) PATH=$(PARITY_PYTHON_BIN):$(PATH) $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_python_production_bridge \
		python_mint_auto_writes_body_discharge_bridge
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_rust_production_bridge \
		rust_mint_auto_writes_body_discharge_bridge_from_real_lifters
	@echo "--- prove parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_java_production_bridge \
		java_production_path_uses_checked_in_java_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_go_production_bridge \
		go_production_path_double_discharges_and_mints_witness
	PYTHON=$(PARITY_PYTHON) PATH=$(PARITY_PYTHON_BIN):$(PATH) $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_python_production_bridge \
		python_production_path_uses_checked_in_python_double_registration
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_rust_production_bridge \
		rust_production_path_double_discharges_and_mints_witness
	@echo "--- contradiction parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_java_production_bridge \
		java_production_path_checked_in_fixture_refuses_planted_contradictory_implication
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_go_production_bridge \
		go_production_path_refuses_planted_contradictory_implication
	PYTHON=$(PARITY_PYTHON) PATH=$(PARITY_PYTHON_BIN):$(PATH) $(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_python_production_bridge \
		python_production_path_refuses_planted_contradictory_implication
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_rust_production_bridge \
		rust_production_path_refuses_planted_contradictory_implication
	@echo "==== cross-language-proof-parity: PASS ===="

.PHONY: cross-language-proof-parity-extra
cross-language-proof-parity-extra: build-ts build-zig build-scala
	@echo "=== Extra proof parity lanes: TypeScript, Zig, Scala, Swift recognizer ==="
	@echo "--- extra emit parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_typescript_vitest \
		emit_typescript_vitest_dispatches_real_emitter_and_vitest_checks_output
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_scala_scalatest \
		emit_scala_scalatest_dispatches_real_emitter_and_scala_cli_checks_output
	@echo "--- extra recognizer parity ---"
	pnpm vitest run implementations/typescript/src/lift/typescript-source/index.test.ts
	if command -v swift >/dev/null 2>&1; then \
		make test-swift-source-lift; \
	else \
		echo "swift not found; skipping Linux Swift parity (covered by macOS swift gate)"; \
	fi
	(cd implementations/zig/provekit-lift-zig-source && zig build test)
	$(SCALA_CLI) test implementations/scala/provekit-lift-scala-source --server=false
	@echo "--- extra prove parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_typescript_production_bridge \
		typescript_production_path_double_discharges_and_mints_witness
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_zig_production_bridge \
		zig_production_path_double_discharges_and_mints_witness
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_scala_production_bridge \
		scala_production_path_double_discharges_and_mints_witness
	@echo "--- extra contradiction parity ---"
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_typescript_production_bridge \
		typescript_production_path_refuses_planted_contradictory_implication
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_zig_production_bridge \
		zig_production_path_refuses_planted_contradictory_implication
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_verify_scala_production_bridge \
		scala_production_path_refuses_planted_contradictory_implication
	@echo "==== cross-language-proof-parity-extra: PASS ===="

.PHONY: cross-language-proof-parity-all
cross-language-proof-parity-all: cross-language-proof-parity cross-language-proof-parity-extra

.PHONY: bootstrap-self-contracts
bootstrap-self-contracts:
	@echo "=== Bootstrap self-contract attestations from live kit artifacts ==="
	$(CARGO) run --release --manifest-path tools/cross-kit-conformance/Cargo.toml -- \
		--profile $(CONFORMANCE_PROFILE) --jobs $(CONFORMANCE_JOBS) \
		--bootstrap-self-contract-attestations

# --- Per-language test suites ------------------------------------------------

.PHONY: test-rust
# The rust integration tests register per-language carriers via
# `register_with_platform_semantics`, which spawns the target kit binary
# over JSON-RPC (PEP 1.7.0) to fetch the PlatformSemanticsDeclaration. The
# java carrier in particular requires the shaded jar from
# provekit-realize-java-core; without `build-java` first, that jar is
# absent and `lower_java_carrier_registration_points_at_required_fixture_set`
# panics with `Unable to access jarfile provekit-realize-java.jar`.
#
# build-ts (pnpm install) is also required: the bug-zoo smoke tests call
# `pnpm exec tsx` from the repo root and fail with ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL
# if node_modules is absent (fresh worktrees, CI).
test-rust: build-java build-ts build-python
	# The rust realize manifests (.provekit/realize/rust-*) spawn the DEBUG
	# binary `implementations/rust/target/debug/provekit-realize-rust`; the
	# release test build does not produce it, so manifest_audit / migrate tests
	# that query the rust kit over RPC would spawn a stale/missing binary and
	# see empty bindings. Build it first so the kit self-resolves its shim
	# .proof for the audit.
	$(CARGO) build --manifest-path implementations/rust/Cargo.toml -p provekit-realize-rust-core --bin provekit-realize-rust
	@failed=""; \
	$(CARGO) test --no-fail-fast --release --manifest-path implementations/rust/Cargo.toml \
	  || failed="$$failed implementations/rust"; \
	$(CARGO) test --no-fail-fast --release --manifest-path tools/recompute-spec-cids/Cargo.toml \
	  || failed="$$failed tools/recompute-spec-cids"; \
	$(CARGO) test --no-fail-fast --release --manifest-path tools/foundation-keygen/Cargo.toml \
	  || failed="$$failed tools/foundation-keygen"; \
	if [ -n "$$failed" ]; then echo "test-rust FAIL:$$failed"; exit 1; fi

.PHONY: bug-zoo
bug-zoo:
	@echo "=== Bug Zoo: live ProvekIt receipts ==="
	env -u PROVEKIT_CLI -u PROVEKIT_BUG_ZOO_EXTERNAL_CLI \
		$(CARGO) run --manifest-path menagerie/bug-zoo/Cargo.toml -- --all

.PHONY: python-language-signature
python-language-signature:
	python3 menagerie/python-language-signature/generate_assets.py --check

.PHONY: ruby-language-signature
ruby-language-signature:
	python3 menagerie/ruby-language-signature/generate_assets.py --check

.PHONY: menagerie-zig-language-signature
menagerie-zig-language-signature:
	python3 menagerie/zig-language-signature/generate_assets.py

.PHONY: test-go
test-go:
	@failed=""; \
	(cd implementations/go && go test ./...) \
	  || failed="$$failed implementations/go"; \
	(cd implementations/go/provekit-ir-symbolic && go test ./...) \
	  || failed="$$failed provekit-ir-symbolic"; \
	(cd implementations/go/provekit-self-contracts && go test ./...) \
	  || failed="$$failed provekit-self-contracts"; \
	(cd implementations/go/provekit-lift-go-tests && go test ./...) \
	  || failed="$$failed provekit-lift-go-tests"; \
	(cd implementations/go/provekit-lift-go && go test ./...) \
	  || failed="$$failed provekit-lift-go"; \
	if [ -n "$$failed" ]; then echo "test-go FAIL:$$failed"; exit 1; fi

.PHONY: test-cpp-source-lift
test-cpp-source-lift:
	tools/test-cpp-source-lift.sh

.PHONY: test-cpp
test-cpp: build-cpp test-cpp-source-lift
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
	@failed=""; \
	$(MAKE) -C implementations/c/provekit-ir test || failed="$$failed provekit-ir"; \
	$(MAKE) -C implementations/c/provekit-lift test || failed="$$failed provekit-lift"; \
	$(MAKE) -C implementations/c/provekit-lift-core test || failed="$$failed provekit-lift-core"; \
	$(MAKE) -C implementations/c/provekit-lift-c-sparse test || failed="$$failed provekit-lift-c-sparse"; \
	$(MAKE) -C implementations/c/provekit-lift-c-kernel-doc test || failed="$$failed provekit-lift-c-kernel-doc"; \
	$(MAKE) -C implementations/c/provekit-lift-c-assertions test || failed="$$failed provekit-lift-c-assertions"; \
	$(MAKE) -C implementations/c/provekit-realize-c-core test || failed="$$failed provekit-realize-c-core"; \
	$(MAKE) -C implementations/c/provekit-lift-composition test || failed="$$failed provekit-lift-composition"; \
	$(MAKE) -C implementations/c/provekit-lsp-c test || failed="$$failed provekit-lsp-c"; \
	$(MAKE) -C implementations/c/provekit-self-contracts test || failed="$$failed provekit-self-contracts"; \
	if [ -n "$$failed" ]; then echo "test-c FAIL:$$failed"; exit 1; fi

.PHONY: test-python
test-python: build-python
	@failed=""; \
	(cd implementations/python/provekit-lift-py-tests && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e . pytest && \
		pytest) || failed="$$failed provekit-lift-py-tests"; \
	(cd implementations/python/provekit-emit-python-pytest && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e . pytest && \
		pytest) || failed="$$failed provekit-emit-python-pytest"; \
	(cd implementations/python/provekit-realize-python-core && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e . pytest && \
		pytest) || failed="$$failed provekit-realize-python-core"; \
	(cd implementations/python/provekit-realize-python-sqlite3 && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-realize-python-core -e ../../../examples/provekit-shim-python-sqlite3 -e . pytest && \
		pytest) || failed="$$failed provekit-realize-python-sqlite3"; \
	(cd implementations/python/provekit-realize-python-aiosqlite && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-realize-python-core -e ../../../examples/provekit-shim-python-aiosqlite -e . pytest && \
		pytest) || failed="$$failed provekit-realize-python-aiosqlite"; \
	(cd implementations/python/provekit-realize-python-requests && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-realize-python-core -e ../../../examples/provekit-shim-python-requests -e . pytest && \
		pytest) || failed="$$failed provekit-realize-python-requests"; \
	if [ -n "$$failed" ]; then echo "test-python FAIL:$$failed"; exit 1; fi

.PHONY: test-ruby
test-ruby: build-ruby ruby-language-signature
	cd implementations/ruby && $(RUBY) -S bundle exec rake test

.PHONY: test-php
test-php:
	cd implementations/php && composer install && composer test

.PHONY: test-java
test-java: build-java
	@failed=""; \
	$(MVN) test -q -f implementations/java/provekit-lift-java-core/pom.xml \
	  || failed="$$failed provekit-lift-java-core"; \
	$(MVN) test -q -f implementations/java/pom.xml -pl provekit-realize-java-core -am \
	  || failed="$$failed provekit-realize-java-core"; \
	if [ -n "$$failed" ]; then echo "test-java FAIL:$$failed"; exit 1; fi

.PHONY: test-scala
test-scala: build-scala
	$(CARGO) test --release --manifest-path implementations/rust/Cargo.toml \
		-p provekit-cli --test cmd_emit_scala_scalatest \
		emit_scala_scalatest_dispatches_real_emitter_and_scala_cli_checks_output
	$(SCALA_CLI) test implementations/scala/provekit-lift-scala-source --server=false

.PHONY: test-swift
test-swift: build-swift
	cd implementations/swift && swift test
	cd implementations/swift && swift run conformance
	cd implementations/swift && swift run test-swift-lsp
	cd implementations/swift && swift run test-swift-crypto

.PHONY: test-swift-source-lift
test-swift-source-lift: build-swift
	cd implementations/swift && swift run test-swift-source-lift

.PHONY: test-zig
test-zig:
	cd implementations/zig/provekit-ir && zig build test
	cd implementations/zig/provekit-self-contracts && zig build test
	@echo "test-zig: native substrate (jcs + cbor + ed25519 + envelopes) verified"
	cd implementations/zig/provekit-lift-zig-tests && zig build test
	cd implementations/zig/provekit-lift-zig-tests && zig build
	cd implementations/zig/provekit-lift-zig-source && zig build test
	cd implementations/zig/provekit-lift-zig-source && zig build
	@echo "test-zig: lift-zig-tests and lift-zig-source binary builds verified"
	cd implementations/zig/provekit-lsp-zig && zig build test
	cd implementations/zig/provekit-lsp-zig && zig build
	@echo "test-zig: LSP lifecycle integration test"
	sh implementations/zig/provekit-lsp-zig/test_lsp.sh

.PHONY: build-zig
build-zig:
	cd implementations/zig/provekit-ir && zig build
	cd implementations/zig/provekit-self-contracts && zig build
	cd implementations/zig/provekit-lift-zig-tests && zig build
	cd implementations/zig/provekit-lift-zig-source && zig build
	cd implementations/zig/provekit-lsp-zig && zig build
	cd implementations/zig/provekit-proof-envelope-zig && zig build
	cd implementations/zig/mint-zig-self-contracts && zig build

# NOTE: test-swift is intentionally excluded from test-all: it requires a
# macOS host with the Swift toolchain. Use `make test-swift` on macOS.
#
# test-all is NON-FAIL-FAST: every suite runs regardless of prior failures.
# Failures are collected and reported as a summary at the end.
.PHONY: test-all
test-all:
	@failed=""; \
	for s in test-rust test-go test-ts test-csharp test-python test-ruby test-php test-java test-scala test-c; do \
	  echo ""; \
	  echo "==== $$s ===="; \
	  $(MAKE) $$s || failed="$$failed $$s"; \
	done; \
	echo ""; \
	if [ -n "$$failed" ]; then \
	  echo "==== test-all FAIL:$$failed ===="; \
	  exit 1; \
	fi; \
	echo "==== test-all: PASS ===="

# --- CI alias ----------------------------------------------------------------

.PHONY: ci
ci: check-cargo-entrypoint check-cross-language-proof-parity-scope conformance cross-language-proof-parity test-all
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
	$(CARGO_LOCAL) clean --manifest-path implementations/rust/Cargo.toml
	$(CARGO_LOCAL) clean --manifest-path tools/recompute-spec-cids/Cargo.toml
	$(CARGO_LOCAL) clean --manifest-path tools/foundation-keygen/Cargo.toml
	rm -rf implementations/cpp/target
	rm -rf implementations/csharp/Provekit.*/bin implementations/csharp/Provekit.*/obj
	rm -rf node_modules
	cd implementations/go/provekit-self-contracts && rm -f mint-go-self-contracts
	rm -f implementations/*/blake3-512:*.proof
	rm -f blake3-512:*.proof
