# ProvekIt: top-level orchestrator
#
# Each kit owns its native build tool; this Makefile is glue, not a build
# system. `make ci` runs the acid test: drop provekit into a project and
# prove correctness with zero code changes.
#
# Mainline targets:
#   make help: print this help
#   make ci: check-cargo-entrypoint + the acid test (test-all)
#   make test-all: the acid test -- test-rust + test-python
#
# `test-rust` runs the rust workspace (including the crate-pair inheritance
# E2E) and exercises the java / ts / python realize kits over RPC; `test-python`
# runs the python kit including the numpy proof. Other per-language suites
# (test-go / test-ts / test-java / ...) exist but are not part of the gate.

.DEFAULT_GOAL := help

PROVEKIT := implementations/rust/target/release/provekit
RUBY ?= $(shell for p in /usr/local/opt/ruby/bin/ruby /opt/homebrew/opt/ruby/bin/ruby /usr/local/bin/ruby /opt/homebrew/bin/ruby; do if [ -x "$$p" ]; then echo "$$p"; exit; fi; done; command -v ruby || echo ruby)
PYTHON ?= $(shell command -v python3 || echo python3)
PIP ?= pip3 --python $(PYTHON)
MVN ?= mvn
LOCAL_BIN ?= /tmp/provekit-local-bin
SCALA_CLI ?= scala-cli
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
	@echo "  make ci             check-cargo-entrypoint + the acid test (test-all)"
	@echo "  make test-all       the acid test: test-rust + test-python"
	@echo ""
	@echo "Per-language build:"
	@echo "  make build-rust     cargo build --release (workspace)"
	@echo "  make build-java     mvn package + install provekit-lsp-java to ~/.local/bin"
	@echo "  make build-python   pip-install Python realize kits and shim packages"
	@echo "  make build-<lang>   go / cpp / csharp / ruby / scala / c / swift"
	@echo ""
	@echo "Per-language test:"
	@echo "  make test-rust  test-python   (the proven provers)"
	@echo "  make test-<lang>              go / ts / csharp / ruby / php / java / scala / c / swift"
	@echo ""
	@echo "Self-lift experiments:"
	@echo "  make self-lift-canonicalizer  run provekit-lift against the canonicalizer crate"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean          remove build artifacts"

# --- Per-language builds -----------------------------------------------------

# Build every kit's binaries. Useful before `make conformance` or before
# spawning `provekit-linkerd` (which subprocesses kit lifters at lift
# time). Each kit's build target is independent; failures stay isolated.
# NOTE: build-swift is intentionally excluded: it requires a macOS host
# with the Swift toolchain and is not run by Linux CI. Use `make build-swift`
# directly on macOS.
.PHONY: build-all
build-all: build-rust build-cpp build-go build-csharp build-java build-python build-ruby build-scala

.PHONY: build-rust
build-rust:
	$(call CARGO_SYNC_BINS,provekit provekit-lift) build --release --manifest-path implementations/rust/Cargo.toml

.PHONY: build-rust-cli
build-rust-cli:
	$(call CARGO_SYNC_BINS,provekit) build --release --manifest-path implementations/rust/Cargo.toml -p provekit-cli

.PHONY: build-cpp
build-cpp:
	tools/build-cpp-lift.sh
	tools/build-cpp-source-lift.sh
	tools/build-cpp-lsp.sh

.PHONY: build-go
build-go:
	cd implementations/go && go build ./...
	cd implementations/go/provekit-ir-symbolic && go build ./...
	cd implementations/go/provekit-lift-go-tests && go build ./...
	cd implementations/go/provekit-lift-go && go build ./...


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

.PHONY: build-java
build-java:
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
		-e implementations/python/provekit-realize-python-core
	# The rust integration suite spawns the python lifter over RPC
	# (python3 -m provekit_lift_py_tests...). Install the lift packages into the
	# same interpreter so those cross-language tests find it.
	$(PIP) install --quiet --no-cache-dir \
		-e implementations/python/provekit-lift-py-tests \
		-e implementations/python/provekit-lift-python-source \
		-e implementations/python/provekit-lift-py-pytest-witness

.PHONY: build-scala
build-scala:
	$(SCALA_CLI) compile implementations/scala/provekit-emit-scala-scalatest --server=false --scalac-option -deprecation
	$(SCALA_CLI) compile implementations/scala/provekit-lift-scala-source --server=false --scalac-option -deprecation

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

.PHONY: check-cargo-entrypoint
check-cargo-entrypoint:
	tools/check-cargo-entrypoint.sh

.PHONY: test-rust
# The rust integration tests register per-language carriers via
# `register_with_platform_semantics`, which spawns the target kit binary
# over JSON-RPC (PEP 1.7.0) to fetch the PlatformSemanticsDeclaration. The
# java carrier in particular requires the shaded jar from
# provekit-realize-java-core; without `build-java` first, that jar is
# absent and `lower_java_carrier_registration_points_at_required_fixture_set`
# panics with `Unable to access jarfile provekit-realize-java.jar`.
test-rust: build-java build-python
	@failed=""; \
	$(CARGO) test --no-fail-fast --release --manifest-path implementations/rust/Cargo.toml \
	  || failed="$$failed implementations/rust"; \
	if [ -n "$$failed" ]; then echo "test-rust FAIL:$$failed"; exit 1; fi

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
	(cd implementations/python/provekit-lift-python-source && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-lift-py-tests -e . pytest blake3 && \
		pytest) || failed="$$failed provekit-lift-python-source"; \
	(cd implementations/python/provekit-lift-py-pytest-witness && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-lift-py-tests -e . pytest pynacl blake3 cbor2 && \
		pytest) || failed="$$failed provekit-lift-py-pytest-witness"; \
	(cd implementations/python/provekit-lift-py-numpy-testing && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-lift-py-tests -e ../provekit-lift-python-source -e ../provekit-lift-py-pytest-witness -e . pytest numpy blake3 pynacl cbor2 && \
		pytest) || failed="$$failed provekit-lift-py-numpy-testing"; \
	(cd implementations/python/provekit-lift-py-pandas-testing && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-lift-py-tests -e . pytest && \
		pytest) || failed="$$failed provekit-lift-py-pandas-testing"; \
	(cd implementations/python/provekit-lift-py-sklearn-testing && \
		python3 -m venv .venv && \
		. .venv/bin/activate && \
		python -m pip install --quiet -e ../provekit-lift-py-tests -e . pytest && \
		pytest) || failed="$$failed provekit-lift-py-sklearn-testing"; \
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
	cd implementations/zig/provekit-lift-zig-tests && zig build
	cd implementations/zig/provekit-lift-zig-source && zig build
	cd implementations/zig/provekit-lsp-zig && zig build
	cd implementations/zig/provekit-proof-envelope-zig && zig build

# NOTE: test-swift is intentionally excluded from test-all: it requires a
# macOS host with the Swift toolchain. Use `make test-swift` on macOS.
#
# The acid test: the two suites that actually prove real code with zero
# changes. `test-rust` runs the rust workspace (including the crate-pair
# inheritance E2E) and exercises the java / ts / python realize kits over RPC;
# `test-python` runs the python kit including the numpy proof. NON-FAIL-FAST:
# both run regardless of prior failure; results summarize at the end.
.PHONY: test-all
test-all:
	@failed=""; \
	for s in test-rust test-python; do \
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
ci: check-cargo-entrypoint test-all
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
	rm -rf implementations/cpp/target
	rm -rf implementations/csharp/Provekit.*/bin implementations/csharp/Provekit.*/obj
	rm -rf node_modules
	rm -f implementations/*/blake3-512:*.proof
	rm -f blake3-512:*.proof
